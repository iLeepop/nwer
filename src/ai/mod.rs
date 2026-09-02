mod actions;
mod bridge;
mod context_ref;
mod effect;
mod host;
mod intent;
mod mutator;
mod prompts;
mod provider;
mod tools;
mod session;
mod ui_command;
mod ui_state;
mod workspace_mutator;

pub use session::{
    truncate_llm_history, AiSession, AiSessionManager, AiSessionSummary, StoredLeanFocus,
    MAX_LLM_HISTORY_MESSAGES, MAX_SESSIONS_PER_PROJECT,
};

pub use actions::action_prompt;
pub use bridge::{
    build_llm, chapter_context_from_app, hydrate_shared_ctx, lean_context_from_app,
    lean_focus_changed, provider_from_settings, resolve_max_tokens, stored_lean_focus_from_lean,
    validate_ai_ready, MAX_TOKENS_HIGH,
};
pub use context_ref::{
    format_attached_refs, push_unique, AiContextKind, AiContextRef,
};
pub use effect::{
    apply_all, apply_proposal, discard_all, discard_proposal, summarize_intent, EffectPolicy,
    Proposal, ProposalStore,
};
pub use ui_command::AiUiCommand;
pub use ui_state::{AiAgentKind, AiChatMessage, AiChatRole, AiMaxTokenTier};
pub use host::{
    compose_user_message_for_turn, format_lean_context, AiSessionHost, LeanContext, LeanFocus,
    LeanSelection,
};
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
