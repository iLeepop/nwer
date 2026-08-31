use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Narration,
    Aside,
    Dialogue,
    Thought,
    SceneBreak,
    Note,
}

impl BlockType {
    pub fn counts_toward_word_total(self) -> bool {
        matches!(
            self,
            BlockType::Narration | BlockType::Aside | BlockType::Dialogue | BlockType::Thought
        )
    }

    /// 对话与心理活动可指定人物名称（复用 `speaker` 字段）。
    pub fn allows_speaker(self) -> bool {
        matches!(self, BlockType::Dialogue | BlockType::Thought)
    }

    /// 计入底栏「对话数」的块类型（对话 + 心理活动）。
    pub fn counts_as_dialogue_stat(self) -> bool {
        matches!(self, BlockType::Dialogue | BlockType::Thought)
    }

    pub fn label(self) -> &'static str {
        match self {
            BlockType::Narration => "叙述",
            BlockType::Aside => "旁白",
            BlockType::Dialogue => "对话",
            BlockType::Thought => "心理活动",
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

    pub fn new_thought(
        content: impl Into<String>,
        speaker: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            block_type: BlockType::Thought,
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

    #[test]
    fn aside_and_thought_roundtrip_and_labels() {
        let now = Utc.with_ymd_and_hms(2026, 8, 30, 8, 0, 0).unwrap();
        let aside = Block::new(BlockType::Aside, "画外音。", now);
        let thought = Block::new_thought("他在想什么？", "李四", now);

        let aside_json = serde_json::to_string(&aside).unwrap();
        assert!(aside_json.contains(r#""type":"aside""#));
        assert_eq!(serde_json::from_str::<Block>(&aside_json).unwrap(), aside);
        assert!(!BlockType::Aside.allows_speaker());
        assert_eq!(BlockType::Aside.label(), "旁白");

        let thought_json = serde_json::to_string(&thought).unwrap();
        assert!(thought_json.contains(r#""type":"thought""#));
        let parsed: Block = serde_json::from_str(&thought_json).unwrap();
        assert_eq!(parsed, thought);
        assert!(BlockType::Thought.allows_speaker());
        assert!(BlockType::Thought.counts_as_dialogue_stat());
        assert_eq!(BlockType::Thought.label(), "心理活动");
    }

    #[test]
    fn word_total_includes_aside_and_thought() {
        assert!(BlockType::Aside.counts_toward_word_total());
        assert!(BlockType::Thought.counts_toward_word_total());
        assert!(!BlockType::Aside.counts_as_dialogue_stat());
    }
}
