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

    pub fn is_empty(&self) -> bool {
        self.proposals.is_empty()
    }

    pub fn push(&mut self, proposal: Proposal) {
        self.proposals.push(proposal);
    }

    pub fn get(&self, intent_id: Uuid) -> Option<&Proposal> {
        self.proposals
            .iter()
            .find(|p| p.intent.intent_id() == intent_id)
    }

    pub fn get_mut(&mut self, intent_id: Uuid) -> Option<&mut Proposal> {
        self.proposals
            .iter_mut()
            .find(|p| p.intent.intent_id() == intent_id)
    }

    pub fn remove(&mut self, intent_id: Uuid) -> Option<Proposal> {
        let idx = self
            .proposals
            .iter()
            .position(|p| p.intent.intent_id() == intent_id)?;
        Some(self.proposals.remove(idx))
    }

    pub fn intent_ids(&self) -> Vec<Uuid> {
        self.proposals
            .iter()
            .map(|p| p.intent.intent_id())
            .collect()
    }

    pub fn clear(&mut self) {
        self.proposals.clear();
    }
}

/// 将提案经 ProjectMutator 落盘；成功则移出队列。
/// `ReplaceBlocks` 目标缺失时标 `stale` 并返回 Err，不部分应用。
pub fn apply_proposal<M: ProjectMutator>(
    intent_id: Uuid,
    store: &mut ProposalStore,
    mutator: &mut M,
) -> anyhow::Result<()> {
    let intent = store
        .get(intent_id)
        .ok_or_else(|| anyhow::anyhow!("proposal {intent_id} not found"))?
        .intent
        .clone();
    let is_replace = matches!(intent, AiIntent::ReplaceBlocks { .. });

    match mutator.apply(&intent) {
        Ok(()) => {
            store.remove(intent_id);
            Ok(())
        }
        Err(e) => {
            if is_replace {
                if let Some(p) = store.get_mut(intent_id) {
                    p.stale = true;
                }
            }
            Err(e)
        }
    }
}

/// 放弃单条提案。
pub fn discard_proposal(intent_id: Uuid, store: &mut ProposalStore) -> anyhow::Result<()> {
    store
        .remove(intent_id)
        .ok_or_else(|| anyhow::anyhow!("proposal {intent_id} not found"))?;
    Ok(())
}

/// 按队列顺序应用全部提案；遇错即停。
pub fn apply_all<M: ProjectMutator>(
    store: &mut ProposalStore,
    mutator: &mut M,
) -> anyhow::Result<()> {
    let ids = store.intent_ids();
    for id in ids {
        apply_proposal(id, store, mutator)?;
    }
    Ok(())
}

/// 清空提案队列。
pub fn discard_all(store: &mut ProposalStore) {
    store.clear();
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

    #[test]
    fn apply_proposal_success() {
        let mut ctx = crate::ai::SharedAiCtx::with_sample_chapter(false);
        let chapter_id = Uuid::from_u128(1);
        let intent_id = Uuid::from_u128(10);
        let initial_len = ctx.mutator.chapter_blocks(chapter_id).unwrap().len();
        ctx.proposals.push(Proposal {
            intent: AiIntent::CreateBlock {
                intent_id,
                chapter_id,
                block_type: BlockType::Narration,
                content: "新段落".into(),
                speaker: None,
                after_block_id: None,
            },
            stale: false,
        });

        apply_proposal(intent_id, &mut ctx.proposals, &mut ctx.mutator).unwrap();

        assert!(ctx.proposals.is_empty());
        assert_eq!(
            ctx.mutator.chapter_blocks(chapter_id).unwrap().len(),
            initial_len + 1
        );
    }

    #[test]
    fn apply_proposal_marks_stale_on_conflict() {
        let mut ctx = crate::ai::SharedAiCtx::with_sample_chapter(false);
        let chapter_id = Uuid::from_u128(1);
        let target_id = ctx.mutator.chapter_blocks(chapter_id).unwrap()[0].id;
        let intent_id = Uuid::from_u128(20);
        let now = chrono::Utc::now();
        ctx.proposals.push(Proposal {
            intent: AiIntent::ReplaceBlocks {
                intent_id,
                chapter_id,
                target_ids: vec![target_id],
                blocks: vec![crate::models::Block::new(
                    BlockType::Narration,
                    "改写后",
                    now,
                )],
            },
            stale: false,
        });

        // 用户侧已把目标块替换掉，原 target_id 不再存在
        ctx.mutator
            .apply(&AiIntent::ReplaceBlocks {
                intent_id: Uuid::from_u128(21),
                chapter_id,
                target_ids: vec![target_id],
                blocks: vec![crate::models::Block::new(
                    BlockType::Narration,
                    "用户已改",
                    now,
                )],
            })
            .unwrap();

        let err = apply_proposal(intent_id, &mut ctx.proposals, &mut ctx.mutator).unwrap_err();
        assert!(
            err.to_string().contains("not found") || err.to_string().contains("stale"),
            "unexpected error: {err}"
        );
        let leftover = ctx.proposals.get(intent_id).expect("proposal kept");
        assert!(leftover.stale, "missing target must mark stale");
    }

    #[test]
    fn discard_proposal_removes() {
        let mut ctx = crate::ai::SharedAiCtx::with_sample_chapter(false);
        let chapter_id = Uuid::from_u128(1);
        let intent_id = Uuid::from_u128(30);
        let initial_len = ctx.mutator.chapter_blocks(chapter_id).unwrap().len();
        ctx.proposals.push(Proposal {
            intent: AiIntent::CreateBlock {
                intent_id,
                chapter_id,
                block_type: BlockType::Narration,
                content: "不该落盘".into(),
                speaker: None,
                after_block_id: None,
            },
            stale: false,
        });

        discard_proposal(intent_id, &mut ctx.proposals).unwrap();

        assert!(ctx.proposals.is_empty());
        assert_eq!(
            ctx.mutator.chapter_blocks(chapter_id).unwrap().len(),
            initial_len
        );
    }

    #[test]
    fn apply_all_applies_queue() {
        let mut ctx = crate::ai::SharedAiCtx::with_sample_chapter(false);
        let chapter_id = Uuid::from_u128(1);
        let initial_len = ctx.mutator.chapter_blocks(chapter_id).unwrap().len();
        ctx.proposals.push(Proposal {
            intent: AiIntent::CreateBlock {
                intent_id: Uuid::from_u128(40),
                chapter_id,
                block_type: BlockType::Narration,
                content: "A".into(),
                speaker: None,
                after_block_id: None,
            },
            stale: false,
        });
        ctx.proposals.push(Proposal {
            intent: AiIntent::CreateBlock {
                intent_id: Uuid::from_u128(41),
                chapter_id,
                block_type: BlockType::Narration,
                content: "B".into(),
                speaker: None,
                after_block_id: None,
            },
            stale: false,
        });

        apply_all(&mut ctx.proposals, &mut ctx.mutator).unwrap();

        assert!(ctx.proposals.is_empty());
        assert_eq!(
            ctx.mutator.chapter_blocks(chapter_id).unwrap().len(),
            initial_len + 2
        );
    }

    #[test]
    fn discard_all_clears_queue() {
        let mut store = ProposalStore::default();
        store.push(Proposal {
            intent: sample_create_block(),
            stale: false,
        });
        store.push(Proposal {
            intent: AiIntent::CreateBlock {
                intent_id: Uuid::from_u128(50),
                chapter_id: Uuid::nil(),
                block_type: BlockType::Narration,
                content: "x".into(),
                speaker: None,
                after_block_id: None,
            },
            stale: false,
        });

        discard_all(&mut store);

        assert!(store.is_empty());
    }
}
