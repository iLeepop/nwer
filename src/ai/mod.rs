mod actions;
mod bridge;
mod effect;
mod host;
mod intent;
mod mutator;
mod prompts;
mod provider;
mod tools;
mod ui_command;
mod ui_state;
mod workspace_mutator;

pub use actions::action_prompt;
pub use bridge::{
    build_llm, chapter_context_from_app, hydrate_shared_ctx, lean_context_from_app,
    provider_from_settings, resolve_max_tokens, validate_ai_ready, MAX_TOKENS_HIGH,
};
pub use effect::{
    apply_all, apply_proposal, discard_all, discard_proposal, summarize_intent, EffectPolicy,
    Proposal, ProposalStore,
};
pub use ui_command::AiUiCommand;
pub use ui_state::{AiAgentKind, AiChatMessage, AiChatRole, AiMaxTokenTier, AiUiState};
pub use host::{format_lean_context, AiSessionHost, LeanContext, LeanFocus, LeanSelection};
pub use intent::*;
pub use mutator::{InMemoryMutator, ProjectMutator};
pub use workspace_mutator::WorkspaceMutator;
pub use prompts::{
    compose_system_prompt, SYSTEM_DIRECTOR_PROMPT, SYSTEM_REVIEWER_PROMPT, SYSTEM_WRITER_PROMPT,
};
pub use provider::{
    AiAction, AiProvider, AiRequest, AiResponse, ChapterContext, ProjectContext, StubAiProvider,
};
pub use tools::{build_all_tools, build_read_tools, SharedAiCtx, SharedCtx};
