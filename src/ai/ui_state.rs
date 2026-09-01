use crate::ai::context_ref::AiContextRef;
use crate::ai::{InMemoryMutator, ProposalStore};

/// 对话气泡角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiChatRole {
    User,
    Assistant,
}

/// 面板中的一条对话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiChatMessage {
    pub role: AiChatRole,
    pub text: String,
}

/// 会话级 max_tokens 档位（不持久化；重启默认 Auto）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

/// AI 面板会话状态（不持久化）。
#[derive(Debug, Clone)]
pub struct AiUiState {
    pub auto_apply: bool,
    /// 输出长度档位：自动 / 低 / 高。
    pub max_token_tier: AiMaxTokenTier,
    /// 当前 Agent 角色。
    pub agent: AiAgentKind,
    /// 拖入输入坞的上下文芯片（发送后清空）。
    pub context_refs: Vec<AiContextRef>,
    pub messages: Vec<AiChatMessage>,
    pub proposals: ProposalStore,
    /// 提案列表是否展开（顶栏始终可见；默认展开）。
    pub proposals_expanded: bool,
    pub busy: bool,
    /// 正在流式接收助手回复（最后一条 Assistant 为当前气泡）。
    pub streaming: bool,
    pub status_message: Option<String>,
    /// 无项目目录时提案应用的内存镜像。
    pub mutator: InMemoryMutator,
}

impl Default for AiUiState {
    fn default() -> Self {
        Self {
            auto_apply: false,
            max_token_tier: AiMaxTokenTier::Auto,
            agent: AiAgentKind::Default,
            context_refs: Vec::new(),
            messages: Vec::new(),
            proposals: ProposalStore::default(),
            proposals_expanded: true,
            busy: false,
            streaming: false,
            status_message: None,
            mutator: InMemoryMutator::new(),
        }
    }
}
