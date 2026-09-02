use serde::{Deserialize, Serialize};

/// 对话气泡角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiChatRole {
    User,
    Assistant,
}

/// 面板中的一条对话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiChatMessage {
    pub role: AiChatRole,
    pub text: String,
}

/// 会话级 max_tokens 档位（不持久化；重启默认 Auto）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AiMaxTokenTier {
    #[default]
    Auto,
    Low,
    High,
}

impl AiMaxTokenTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动",
            Self::Low => "低",
            Self::High => "高",
        }
    }

    pub fn all() -> [Self; 3] {
        [Self::Auto, Self::Low, Self::High]
    }
}

/// 会话级 Agent 角色（不持久化；重启默认 Default）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AiAgentKind {
    #[default]
    Default,
    Writer,
    Reviewer,
    Director,
}

impl AiAgentKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "默认",
            Self::Writer => "写手",
            Self::Reviewer => "审查",
            Self::Director => "导演",
        }
    }

    pub fn all() -> [Self; 4] {
        [Self::Default, Self::Writer, Self::Reviewer, Self::Director]
    }
}
