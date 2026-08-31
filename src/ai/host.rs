use std::sync::{Arc, Mutex};

use rai_l::agent::agent::FunctionCallAgent;
use rai_l::agent::config::Config;
use rai_l::agent::core::Agent;
use rai_l::llm::{RaiLLM, Think};
use uuid::Uuid;

use crate::ai::actions::action_prompt;
use crate::ai::provider::{ChapterContext, ProjectContext};
use crate::ai::tools::{build_all_tools, SharedCtx};
use crate::ai::{AiAction, SharedAiCtx};
use crate::models::Block;

/// 每次 run 注入的瘦上下文：项目元信息 + 焦点摘要 + 选区（非全文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanContext {
    pub project: ProjectContext,
    pub focus: Option<LeanFocus>,
    pub selection: Vec<LeanSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanFocus {
    Chapter { id: Uuid, title: String },
    Script { id: Uuid, title: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanSelection {
    pub block_id: Uuid,
    pub block_type: String,
    pub text: String,
}

impl LeanContext {
    pub fn from_chapter(
        project: ProjectContext,
        chapter: ChapterContext,
        selected_blocks: &[Block],
    ) -> Self {
        Self {
            project,
            focus: Some(LeanFocus::Chapter {
                id: chapter.chapter_id,
                title: chapter.title,
            }),
            selection: selected_blocks
                .iter()
                .map(|b| LeanSelection {
                    block_id: b.id,
                    block_type: b.block_type.label().to_string(),
                    text: b.content.clone(),
                })
                .collect(),
        }
    }
}

/// 组装瘦上下文、动作提示，并驱动 raiL FunctionCallAgent。
pub struct AiSessionHost<L> {
    pub ctx: SharedCtx,
    lean: LeanContext,
    llm: L,
}

impl<L: Think + Clone + Send> AiSessionHost<L> {
    pub fn from_llm(llm: L, ctx: SharedAiCtx, lean: LeanContext) -> Self {
        Self {
            ctx: Arc::new(Mutex::new(ctx)),
            lean,
            llm,
        }
    }

    pub async fn run(
        &mut self,
        action: AiAction,
        user_prompt: &str,
        max_tokens: u32,
        max_tool_rounds: u32,
    ) -> anyhow::Result<String> {
        let tools = build_all_tools(self.ctx.clone());
        let mut agent = FunctionCallAgent::from_parts(
            "nwer",
            FunctionCallAgent::<RaiLLM>::DEFAULT_SYSTEM_PROMPT,
            Config::default()
                .with_max_token(max_tokens)
                .with_max_tool_rounds(max_tool_rounds.max(1)),
            self.llm.clone(),
            tools,
        );
        let message = compose_user_message(&self.lean, action, user_prompt);
        agent
            .run(message)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

/// 项目元信息 + 焦点摘要 + 选区。
pub fn format_lean_context(lean: &LeanContext) -> String {
    let mut lines = Vec::new();
    lines.push(format!("项目：{}", lean.project.title));
    if !lean.project.style_guide.is_empty() {
        lines.push(format!("风格：{}", lean.project.style_guide));
    }
    if !lean.project.synopsis.is_empty() {
        lines.push(format!("简介：{}", lean.project.synopsis));
    }
    match &lean.focus {
        Some(LeanFocus::Chapter { id, title }) => {
            lines.push(format!("焦点：章节 {title} ({id})"));
        }
        Some(LeanFocus::Script { id, title }) => {
            lines.push(format!("焦点：剧本 {title} ({id})"));
        }
        None => {}
    }
    if !lean.selection.is_empty() {
        lines.push("选区：".into());
        for sel in &lean.selection {
            lines.push(format!(
                "- [{}] {}: {}",
                sel.block_type, sel.block_id, sel.text
            ));
        }
    }
    lines.join("\n")
}

fn compose_user_message(lean: &LeanContext, action: AiAction, user_prompt: &str) -> String {
    let lean_text = format_lean_context(lean);
    let action_text = action_prompt(action);
    let mut parts = Vec::new();
    if !lean_text.is_empty() {
        parts.push(lean_text);
    }
    if !action_text.is_empty() {
        parts.push(action_text.to_string());
    }
    if !user_prompt.is_empty() {
        parts.push(user_prompt.to_string());
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rai_l::llm::{
        ChatCompletionRequestMessage, ChatCompletionTools, Think, ThinkOutput, ToolCall,
    };
    use std::collections::VecDeque;
    use std::error::Error;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    use crate::ai::tools::SharedAiCtx;
    use crate::ai::AiAction;
    use crate::models::{Block, BlockType};

    /// 脚本化 fake LLM：按调用顺序吐出预设输出（raiL agent 包内 FakeLLM 对 nwer 不可见）。
    #[derive(Clone)]
    struct FakeLLM {
        script: Arc<Mutex<VecDeque<ThinkOutput>>>,
        received: Arc<Mutex<Vec<Vec<ChatCompletionRequestMessage>>>>,
    }

    impl FakeLLM {
        fn new(script: Vec<ThinkOutput>) -> Self {
            Self {
                script: Arc::new(Mutex::new(script.into())),
                received: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl Think for FakeLLM {
        fn think(
            self,
            messages: Vec<ChatCompletionRequestMessage>,
            _tools: &[ChatCompletionTools],
            _max_tokens: u32,
        ) -> impl Future<Output = Result<ThinkOutput, Box<dyn Error>>> + Send {
            async move {
                self.received.lock().unwrap().push(messages);
                let next = self.script.lock().unwrap().pop_front().unwrap();
                Ok(next)
            }
        }
    }

    fn lean_ctx() -> LeanContext {
        LeanContext {
            project: ProjectContext {
                project_id: Uuid::nil(),
                title: "测试项目".into(),
                style_guide: "简洁文风".into(),
                synopsis: "样例简介".into(),
            },
            focus: Some(LeanFocus::Chapter {
                id: Uuid::from_u128(1),
                title: "第一章".into(),
            }),
            selection: vec![LeanSelection {
                block_id: Uuid::from_u128(9),
                block_type: "叙述".into(),
                text: "开篇选区".into(),
            }],
        }
    }

    #[test]
    fn format_lean_context_includes_meta_focus_and_selection() {
        let text = format_lean_context(&lean_ctx());
        assert!(text.contains("测试项目"), "应含项目标题: {text}");
        assert!(
            text.contains("简洁文风") || text.contains("风格"),
            "应含风格: {text}"
        );
        assert!(
            text.contains("样例简介") || text.contains("简介"),
            "应含简介: {text}"
        );
        assert!(text.contains("第一章"), "应含焦点标题: {text}");
        assert!(text.contains("开篇选区"), "应含选区文本: {text}");
    }

    #[tokio::test]
    async fn host_run_executes_tool_then_replies() {
        let chapter_id = Uuid::from_u128(1);
        let args = serde_json::json!({
            "chapter_id": chapter_id,
            "block_type": "narration",
            "content": "新段落",
        })
        .to_string();

        let fake = FakeLLM::new(vec![
            ThinkOutput {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "c1".into(),
                    name: "create_block".into(),
                    arguments: args,
                }],
            },
            ThinkOutput {
                content: Some("已提案创建块".into()),
                tool_calls: vec![],
            },
        ]);
        let mut host = AiSessionHost::from_llm(fake, SharedAiCtx::new(false), lean_ctx());
        let reply = host
            .run(
                AiAction::Chat,
                "写一段叙述",
                RaiLLM::MAX_TOKENS_NORMAL,
                8,
            )
            .await
            .unwrap();
        assert!(
            reply.contains("提案") || reply.contains("已"),
            "unexpected reply: {reply}"
        );
        assert_eq!(host.ctx.lock().unwrap().proposals.len(), 1);
    }

    #[test]
    fn lean_context_from_chapter_maps_selection() {
        let now = Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap();
        let block = Block::new(BlockType::Narration, "正文", now);
        let lean = LeanContext::from_chapter(
            ProjectContext {
                project_id: Uuid::nil(),
                title: "P".into(),
                style_guide: String::new(),
                synopsis: String::new(),
            },
            ChapterContext {
                chapter_id: Uuid::from_u128(1),
                title: "第一章".into(),
                status: "draft".into(),
            },
            &[block.clone()],
        );
        match lean.focus {
            Some(LeanFocus::Chapter { title, .. }) => assert_eq!(title, "第一章"),
            other => panic!("expected chapter focus, got {other:?}"),
        }
        assert_eq!(lean.selection.len(), 1);
        assert_eq!(lean.selection[0].text, "正文");
        assert_eq!(lean.selection[0].block_id, block.id);
    }
}
