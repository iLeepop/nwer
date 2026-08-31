use crate::ai::AiIntent;

/// 将 AiIntent 写入项目存储的可插拔后端。
pub trait ProjectMutator {
    fn apply(&mut self, intent: &AiIntent) -> anyhow::Result<()>;
}
