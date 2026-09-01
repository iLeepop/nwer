//! Agent 系统提示词：硬编码角色段 + 可选项目追加段。

use crate::ai::ui_state::AiAgentKind;
use crate::models::AiContext;

/// 系统默认「写手」角色提示词（代码常量，非 UI 可编辑）。
pub const SYSTEM_WRITER_PROMPT: &str = "\
你是小说写手助手。根据用户意图起草、续写或改写正文与剧本内容。\
保持文风一致，优先给出可直接落盘的文本，并在需要时使用工具提案修改。";

/// 系统默认「审查」角色提示词。
pub const SYSTEM_REVIEWER_PROMPT: &str = "\
你是文稿审查助手。检查逻辑漏洞、文风不一、人物口吻与节奏问题。\
给出具体、可执行的修改建议；需要改稿时用工具提出明确提案。";

/// 系统默认「导演」角色提示词。
pub const SYSTEM_DIRECTOR_PROMPT: &str = "\
你是叙事导演助手。关注结构、场景节奏、冲突升级与视角调度。\
帮助规划章节/剧本走向，必要时用工具调整大纲或结构相关内容。";

/// 按 Agent 种类拼装系统提示词。
///
/// - `Default`：原样返回 `default_system`（通常为 FunctionCallAgent 默认系统提示）。
/// - 角色 Agent：系统硬编码角色段 +（若 trim 非空）项目对应提示词。
pub fn compose_system_prompt(
    kind: AiAgentKind,
    project: Option<&AiContext>,
    default_system: &str,
) -> String {
    let (role, project_extra) = match kind {
        AiAgentKind::Default => return default_system.to_string(),
        AiAgentKind::Writer => (
            SYSTEM_WRITER_PROMPT,
            project.map(|p| p.writer_prompt.as_str()),
        ),
        AiAgentKind::Reviewer => (
            SYSTEM_REVIEWER_PROMPT,
            project.map(|p| p.reviewer_prompt.as_str()),
        ),
        AiAgentKind::Director => (
            SYSTEM_DIRECTOR_PROMPT,
            project.map(|p| p.director_prompt.as_str()),
        ),
    };

    match project_extra.map(str::trim).filter(|s| !s.is_empty()) {
        Some(extra) => format!("{role}\n\n{extra}"),
        None => role.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(writer: &str, reviewer: &str, director: &str) -> AiContext {
        AiContext {
            style_guide: String::new(),
            synopsis: String::new(),
            writer_prompt: writer.into(),
            reviewer_prompt: reviewer.into(),
            director_prompt: director.into(),
        }
    }

    #[test]
    fn default_returns_base_system_without_project() {
        let project = ctx("项目写手段", "审查段", "导演段");
        let out = compose_system_prompt(AiAgentKind::Default, Some(&project), "BASE");
        assert_eq!(out, "BASE");
        assert!(!out.contains("项目写手段"));
    }

    #[test]
    fn writer_without_project_uses_system_only() {
        let out = compose_system_prompt(AiAgentKind::Writer, None, "BASE");
        assert_eq!(out, SYSTEM_WRITER_PROMPT);
        assert!(!out.contains("BASE"));
    }

    #[test]
    fn writer_with_empty_project_prompt_uses_system_only() {
        let project = ctx("  \n  ", "x", "y");
        let out = compose_system_prompt(AiAgentKind::Writer, Some(&project), "BASE");
        assert_eq!(out, SYSTEM_WRITER_PROMPT);
    }

    #[test]
    fn writer_appends_nonempty_project_prompt() {
        let project = ctx("古风克制", "", "");
        let out = compose_system_prompt(AiAgentKind::Writer, Some(&project), "BASE");
        assert!(out.starts_with(SYSTEM_WRITER_PROMPT));
        assert!(out.contains("古风克制"));
        assert!(!out.contains("BASE"));
    }

    #[test]
    fn reviewer_and_director_use_matching_fields() {
        let project = ctx("W", "查漏洞", "管节奏");
        let r = compose_system_prompt(AiAgentKind::Reviewer, Some(&project), "BASE");
        let d = compose_system_prompt(AiAgentKind::Director, Some(&project), "BASE");
        assert!(r.contains(SYSTEM_REVIEWER_PROMPT) && r.contains("查漏洞"));
        assert!(d.contains(SYSTEM_DIRECTOR_PROMPT) && d.contains("管节奏"));
        assert!(!r.contains("管节奏"));
        assert!(!d.contains("查漏洞"));
    }
}
