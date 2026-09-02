use std::sync::{Arc, Mutex};

use rai_l::agent::agent::FunctionCallAgent;
use rai_l::agent::config::Config;
use rai_l::agent::core::Agent;
use rai_l::llm::{Message, RaiLLM, Think};
use uuid::Uuid;

use crate::ai::actions::action_prompt;
use crate::ai::provider::{ChapterContext, ProjectContext};
use crate::ai::tools::{build_all_tools, SharedCtx};
use crate::ai::session::StoredLeanFocus;
use crate::ai::{AiAction, SharedAiCtx};
use crate::models::Block;

/// 每次 run 注入的瘦上下文：项目元信息 + 焦点摘要 + 选区（非全文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanContext {
    pub project: ProjectContext,
    pub focus: Option<LeanFocus>,
    pub selection: Vec<LeanSelection>,
    /// 用户拖入 AI 面板的附加引用（瘦元数据）。
    pub attached_refs: Vec<crate::ai::AiContextRef>,
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
            attached_refs: Vec::new(),
        }
    }
}

/// 组装瘦上下文、动作提示，并驱动 raiL FunctionCallAgent。
pub struct AiSessionHost<L> {
    pub ctx: SharedCtx,
    lean: LeanContext,
    llm: L,
    system_prompt: String,
    llm_history: Vec<Message>,
    last_lean_focus: Option<StoredLeanFocus>,
}

impl<L: Think + Clone + Send> AiSessionHost<L> {
    pub fn from_llm(llm: L, ctx: SharedAiCtx, lean: LeanContext) -> Self {
        Self {
            ctx: Arc::new(Mutex::new(ctx)),
            lean,
            llm,
            system_prompt: FunctionCallAgent::<RaiLLM>::DEFAULT_SYSTEM_PROMPT.to_string(),
            llm_history: Vec::new(),
            last_lean_focus: None,
        }
    }

    pub fn with_last_lean_focus(mut self, focus: Option<StoredLeanFocus>) -> Self {
        self.last_lean_focus = focus;
        self
    }

    pub fn with_llm_history(mut self, history: Vec<Message>) -> Self {
        self.llm_history = history;
        self
    }

    pub fn llm_history(&self) -> &[Message] {
        &self.llm_history
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub async fn run(
        &mut self,
        action: AiAction,
        user_prompt: &str,
        max_tokens: u32,
        max_tool_rounds: u32,
    ) -> anyhow::Result<String> {
        self.run_stream(action, user_prompt, max_tokens, max_tool_rounds, |_| {})
            .await
    }

    /// 流式运行：内容增量通过 `on_delta` 回调；最终仍返回完整回复字符串。
    pub async fn run_stream(
        &mut self,
        action: AiAction,
        user_prompt: &str,
        max_tokens: u32,
        max_tool_rounds: u32,
        mut on_delta: impl FnMut(&str) + Send,
    ) -> anyhow::Result<String> {
        let tools = build_all_tools(self.ctx.clone());
        let is_first_turn = self.llm_history.is_empty();
        let current_focus = crate::ai::stored_lean_focus_from_lean(&self.lean);
        let focus_changed =
            !is_first_turn && crate::ai::lean_focus_changed(&self.last_lean_focus, &current_focus);
        let message = compose_user_message_for_turn(
            &self.lean,
            action,
            user_prompt,
            is_first_turn,
            focus_changed,
        );
        let mut agent = FunctionCallAgent::from_parts(
            "nwer",
            self.system_prompt.as_str(),
            Config::default()
                .with_max_token(max_tokens)
                .with_max_tool_rounds(max_tool_rounds.max(1)),
            self.llm.clone(),
            tools,
        );
        for msg in self.llm_history.iter().cloned() {
            agent.add_message(msg);
        }
        let reply = agent
            .run_stream(message, |delta| on_delta(delta))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.llm_history = agent.core.history.clone();
        Ok(reply)
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
    if let Some(attached) = crate::ai::format_attached_refs(&lean.attached_refs) {
        lines.push(attached);
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

/// 首轮或焦点变更时注入 lean context；其余轮仅用户文本。
pub fn compose_user_message_for_turn(
    lean: &LeanContext,
    action: AiAction,
    user_prompt: &str,
    is_first_turn: bool,
    focus_changed: bool,
) -> String {
    if is_first_turn {
        compose_user_message(lean, action, user_prompt)
    } else if focus_changed {
        let lean_text = format_lean_context(lean);
        if lean_text.is_empty() {
            user_prompt.to_string()
        } else {
            format!("{lean_text}\n\n{user_prompt}")
        }
    } else {
        user_prompt.to_string()
    }
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
            attached_refs: Vec::new(),
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

    #[test]
    fn compose_user_message_for_turn_skips_lean_on_follow_up() {
        let lean = lean_ctx();
        let first = compose_user_message_for_turn(&lean, AiAction::Chat, "写一段", true, false);
        assert!(first.contains("测试项目"), "首轮应含 lean: {first}");
        let second = compose_user_message_for_turn(&lean, AiAction::Chat, "继续", false, false);
        assert_eq!(second, "继续");
        let refocus = compose_user_message_for_turn(&lean, AiAction::Chat, "换章了", false, true);
        assert!(refocus.contains("测试项目"), "焦点变更应含 lean: {refocus}");
    }

    #[tokio::test]
    async fn host_preserves_history_across_two_runs() {
        let fake = FakeLLM::new(vec![
            ThinkOutput {
                content: Some("第一轮回复".into()),
                tool_calls: vec![],
            },
            ThinkOutput {
                content: Some("记得你说了开篇".into()),
                tool_calls: vec![],
            },
        ]);
        let mut host = AiSessionHost::from_llm(fake.clone(), SharedAiCtx::new(false), lean_ctx());
        host.run(AiAction::Chat, "开篇选区", RaiLLM::MAX_TOKENS_NORMAL, 8)
            .await
            .unwrap();
        assert_eq!(host.llm_history().len(), 2);

        host.run(AiAction::Chat, "我刚才说了什么", RaiLLM::MAX_TOKENS_NORMAL, 8)
            .await
            .unwrap();

        let batches = fake.received.lock().unwrap();
        assert_eq!(batches.len(), 2);
        let second_batch = &batches[1];
        let has_prior = second_batch
            .iter()
            .any(|m| format!("{m:?}").contains("开篇选区"));
        assert!(has_prior, "第二轮应含第一轮 user 消息: {second_batch:?}");
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
