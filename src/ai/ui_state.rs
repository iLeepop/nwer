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

/// AI 面板会话状态（不持久化）。
#[derive(Debug, Clone)]
pub struct AiUiState {
    pub auto_apply: bool,
    pub messages: Vec<AiChatMessage>,
    pub proposals: ProposalStore,
    pub busy: bool,
    pub status_message: Option<String>,
    /// 无项目目录时提案应用的内存镜像。
    pub mutator: InMemoryMutator,
}

impl Default for AiUiState {
    fn default() -> Self {
        Self {
            auto_apply: false,
            messages: Vec::new(),
            proposals: ProposalStore::default(),
            busy: false,
            status_message: None,
            mutator: InMemoryMutator::new(),
        }
    }
}
