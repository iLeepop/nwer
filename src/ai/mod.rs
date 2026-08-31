mod actions;
mod effect;
mod host;
mod intent;
mod mutator;
mod provider;
mod tools;
mod ui_state;

pub use actions::action_prompt;
pub use effect::{
    apply_all, apply_proposal, discard_all, discard_proposal, summarize_intent, EffectPolicy,
    Proposal, ProposalStore,
};
pub use ui_state::{AiChatMessage, AiChatRole, AiUiState};
pub use host::{format_lean_context, AiSessionHost, LeanContext, LeanFocus, LeanSelection};
pub use intent::*;
pub use mutator::{InMemoryMutator, ProjectMutator};
pub use provider::{
    AiAction, AiProvider, AiRequest, AiResponse, ChapterContext, ProjectContext, StubAiProvider,
};
pub use tools::{build_all_tools, build_read_tools, SharedAiCtx, SharedCtx};
