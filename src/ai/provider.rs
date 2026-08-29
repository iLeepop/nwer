use uuid::Uuid;

use crate::models::{Block, OutlineEntry};

/// AI 调用动作（第一版仅定义边界，不执行真实推理）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiAction {
    Chat,
    Continue,
    Rewrite,
    Expand,
    GenerateDialogue,
    Summarize,
}

/// 项目级上下文摘要，供后续 Agent 适配器组装 prompt。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    pub project_id: Uuid,
    pub title: String,
    pub style_guide: String,
    pub synopsis: String,
}

/// 当前章节上下文摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterContext {
    pub chapter_id: Uuid,
    pub title: String,
    pub status: String,
}

/// 一次 AI 补全请求的数据边界。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRequest {
    pub project_context: ProjectContext,
    pub chapter_context: ChapterContext,
    pub selected_blocks: Vec<Block>,
    pub outline_entries: Vec<OutlineEntry>,
    pub prompt: String,
    pub action: AiAction,
}

/// AI 补全响应的最小边界类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiResponse {
    pub content: String,
}

/// Agent library 与应用模型之间的适配边界。
pub trait AiProvider {
    fn complete(&self, request: AiRequest) -> anyhow::Result<AiResponse>;
}

/// 未配置真实 Provider 时的占位实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct StubAiProvider;

impl AiProvider for StubAiProvider {
    fn complete(&self, _request: AiRequest) -> anyhow::Result<AiResponse> {
        anyhow::bail!("AI provider not configured")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    use crate::models::{BlockType, OutlineCategory};

    fn sample_request() -> AiRequest {
        let now = Utc.with_ymd_and_hms(2026, 8, 30, 1, 0, 0).unwrap();
        AiRequest {
            project_context: ProjectContext {
                project_id: Uuid::nil(),
                title: "示例".into(),
                style_guide: String::new(),
                synopsis: String::new(),
            },
            chapter_context: ChapterContext {
                chapter_id: Uuid::nil(),
                title: "第一章".into(),
                status: "draft".into(),
            },
            selected_blocks: vec![Block::new(BlockType::Narration, "正文", now)],
            outline_entries: vec![OutlineEntry::new("角色甲", OutlineCategory::Character, now)],
            prompt: "继续写下去".into(),
            action: AiAction::Continue,
        }
    }

    #[test]
    fn stub_provider_returns_not_configured_error() {
        let provider = StubAiProvider;
        let err = provider.complete(sample_request()).unwrap_err();
        assert!(
            err.to_string().contains("AI provider not configured"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn ai_request_carries_context_fields() {
        let req = sample_request();
        assert_eq!(req.action, AiAction::Continue);
        assert_eq!(req.prompt, "继续写下去");
        assert_eq!(req.selected_blocks.len(), 1);
        assert_eq!(req.outline_entries.len(), 1);
        assert_eq!(req.project_context.title, "示例");
        assert_eq!(req.chapter_context.title, "第一章");
    }
}
