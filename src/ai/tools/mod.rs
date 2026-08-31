pub mod read;
pub mod write;

use std::sync::{Arc, Mutex};

use rai_l::agent::core::ToolRegister;

use crate::ai::{EffectPolicy, InMemoryMutator, ProposalStore};
use uuid::Uuid;

/// 读/写工具共享的 AI 会话上下文。
pub struct SharedAiCtx {
    pub mutator: InMemoryMutator,
    pub proposals: ProposalStore,
    pub policy: EffectPolicy,
}

impl SharedAiCtx {
    pub fn new(auto_apply: bool) -> Self {
        Self {
            mutator: InMemoryMutator::new(),
            proposals: ProposalStore::default(),
            policy: EffectPolicy::new(auto_apply),
        }
    }

    pub fn with_sample_chapter(auto_apply: bool) -> Self {
        Self {
            mutator: InMemoryMutator::with_sample_chapter(),
            proposals: ProposalStore::default(),
            policy: EffectPolicy::new(auto_apply),
        }
    }

    pub fn apply_proposal(&mut self, intent_id: Uuid) -> anyhow::Result<()> {
        crate::ai::apply_proposal(intent_id, &mut self.proposals, &mut self.mutator)
    }

    pub fn discard_proposal(&mut self, intent_id: Uuid) -> anyhow::Result<()> {
        crate::ai::discard_proposal(intent_id, &mut self.proposals)
    }

    pub fn apply_all(&mut self) -> anyhow::Result<()> {
        crate::ai::apply_all(&mut self.proposals, &mut self.mutator)
    }

    pub fn discard_all(&mut self) {
        crate::ai::discard_all(&mut self.proposals)
    }
}

pub type SharedCtx = Arc<Mutex<SharedAiCtx>>;

pub fn build_all_tools(ctx: SharedCtx) -> ToolRegister {
    let mut reg = ToolRegister::new();
    read::build_read_tools(ctx.clone(), &mut reg);
    write::build_write_tools(ctx, &mut reg);
    reg
}

pub use read::build_read_tools;
