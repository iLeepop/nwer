use crate::ai::AiAction;

/// 结构化动作附加到 user 消息上的短提示（与对话共用同一工具表）。
pub fn action_prompt(action: AiAction) -> &'static str {
    match action {
        AiAction::Chat => "",
        AiAction::Continue => "请在当前焦点后续写下去。可用 create_block 等写工具。",
        AiAction::Expand => "请在当前焦点上扩写。可用 create_block 等写工具。",
        AiAction::GenerateDialogue => "请生成对话。可用 create_block（dialogue）等写工具。",
        AiAction::Rewrite => {
            "请改写当前选区。优先使用 propose_replace_blocks，不要整章重写。"
        }
        AiAction::Summarize => {
            "请先用读工具了解内容再给出摘要。若需写入大纲，使用大纲写工具。"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_prompt_prefers_replace() {
        let p = action_prompt(AiAction::Rewrite);
        let lower = p.to_ascii_lowercase();
        assert!(
            lower.contains("replace") || p.contains("改写") || p.contains("替换"),
            "Rewrite 应提示偏好 replace / 改写选区, got: {p:?}"
        );
    }

    #[test]
    fn chat_prompt_is_empty_or_short() {
        let p = action_prompt(AiAction::Chat);
        assert!(p.chars().count() <= 40, "Chat 提示应为空或简短, got: {p:?}");
    }
}
