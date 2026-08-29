use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Narration,
    Dialogue,
    SceneBreak,
    Note,
}

impl BlockType {
    pub fn counts_toward_word_total(self) -> bool {
        matches!(self, BlockType::Narration | BlockType::Dialogue)
    }

    pub fn label(self) -> &'static str {
        match self {
            BlockType::Narration => "叙述",
            BlockType::Dialogue => "对话",
            BlockType::SceneBreak => "场景分隔",
            BlockType::Note => "备注",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockMeta {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub note: String,
}

impl BlockMeta {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            created_at: now,
            updated_at: now,
            note: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub block_type: BlockType,
    pub content: String,
    pub speaker: Option<String>,
    pub meta: BlockMeta,
}

impl Block {
    pub fn new(block_type: BlockType, content: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::now_v7(),
            block_type,
            content: content.into(),
            speaker: None,
            meta: BlockMeta::new(now),
        }
    }

    pub fn new_dialogue(
        content: impl Into<String>,
        speaker: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            block_type: BlockType::Dialogue,
            content: content.into(),
            speaker: Some(speaker.into()),
            meta: BlockMeta::new(now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn block_roundtrip_json() {
        let now = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let block = Block::new_dialogue("你好。", "张三", now);
        let json = serde_json::to_string_pretty(&block).unwrap();
        let parsed: Block = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, block);
        assert_eq!(parsed.block_type, BlockType::Dialogue);
        assert_eq!(parsed.speaker.as_deref(), Some("张三"));
    }
}
