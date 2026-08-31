use uuid::Uuid;

use crate::ai::{AiIntent, IntentStatus, ToolReceipt};

use super::mutator::ProjectMutator;

/// 待用户确认或稍后应用的 AI 写意图。
pub struct Proposal {
    pub intent: AiIntent,
    pub stale: bool,
}

/// 提案队列：`auto_apply` 关闭时由 EffectPolicy 写入。
#[derive(Default)]
pub struct ProposalStore {
    proposals: Vec<Proposal>,
}

impl ProposalStore {
    pub fn len(&self) -> usize {
        self.proposals.len()
    }

    pub fn push(&mut self, proposal: Proposal) {
        self.proposals.push(proposal);
    }

    pub fn get(&self, intent_id: Uuid) -> Option<&Proposal> {
        self.proposals
            .iter()
            .find(|p| p.intent.intent_id() == intent_id)
    }
}

/// 控制写意图是立即落盘还是先入提案队列。
pub struct EffectPolicy {
    pub auto_apply: bool,
}

impl EffectPolicy {
    pub fn new(auto_apply: bool) -> Self {
        Self { auto_apply }
    }

    pub fn apply<M: ProjectMutator>(
        &self,
        intent: AiIntent,
        store: &mut ProposalStore,
        mutator: &mut M,
    ) -> ToolReceipt {
        let intent_id = intent.intent_id();
        let summary = summarize_intent(&intent);

        if self.auto_apply {
            match mutator.apply(&intent) {
                Ok(()) => ToolReceipt {
                    status: IntentStatus::Applied,
                    intent_id,
                    summary,
                    error: None,
                },
                Err(e) => ToolReceipt {
                    status: IntentStatus::Error,
                    intent_id,
                    summary,
                    error: Some(e.to_string()),
                },
            }
        } else {
            store.push(Proposal {
                intent,
                stale: false,
            });
            ToolReceipt {
                status: IntentStatus::Proposed,
                intent_id,
                summary,
                error: None,
            }
        }
    }
}

fn summarize_intent(intent: &AiIntent) -> String {
    match intent {
        AiIntent::CreateBlock { block_type, .. } => {
            let label = match block_type {
                crate::models::BlockType::Narration => "叙述",
                crate::models::BlockType::Aside => "旁白",
                crate::models::BlockType::Dialogue => "对话",
                crate::models::BlockType::Thought => "心理",
                crate::models::BlockType::SceneBreak => "场景分隔",
                crate::models::BlockType::Note => "注释",
            };
            format!("创建{label}块")
        }
        AiIntent::ReplaceBlocks { .. } => "替换块".into(),
        AiIntent::CreateOutlineEntry { .. } => "创建大纲条目".into(),
        AiIntent::UpdateOutlineEntry { .. } => "更新大纲条目".into(),
        AiIntent::CreateScript { .. } => "创建剧本".into(),
        AiIntent::AppendScriptBlocks { .. } => "追加剧本块".into(),
        AiIntent::UpdateScriptBlock { .. } => "更新剧本块".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::mutator::ProjectMutator;
    use crate::models::BlockType;

    fn sample_create_block() -> AiIntent {
        let id = Uuid::nil();
        AiIntent::CreateBlock {
            intent_id: id,
            chapter_id: id,
            block_type: BlockType::Narration,
            content: "你好".into(),
            speaker: None,
            after_block_id: None,
        }
    }

    struct NoopMutator;

    impl ProjectMutator for NoopMutator {
        fn apply(&mut self, _intent: &AiIntent) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingMutator {
        applied: Vec<AiIntent>,
    }

    impl ProjectMutator for RecordingMutator {
        fn apply(&mut self, intent: &AiIntent) -> anyhow::Result<()> {
            self.applied.push(intent.clone());
            Ok(())
        }
    }

    #[test]
    fn policy_off_stores_proposal() {
        let mut store = ProposalStore::default();
        let policy = EffectPolicy::new(false);
        let intent = sample_create_block();
        let receipt = policy.apply(intent.clone(), &mut store, &mut NoopMutator);
        assert_eq!(receipt.status, IntentStatus::Proposed);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn policy_on_applies_via_mutator() {
        let mut store = ProposalStore::default();
        let policy = EffectPolicy::new(true);
        let mut mutator = RecordingMutator::default();
        let intent = sample_create_block();
        let receipt = policy.apply(intent, &mut store, &mut mutator);
        assert_eq!(receipt.status, IntentStatus::Applied);
        assert_eq!(store.len(), 0);
        assert_eq!(mutator.applied.len(), 1);
    }
}
