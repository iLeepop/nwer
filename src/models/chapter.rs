use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Block;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterMeta {
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "draft".to_string()
}

impl Default for ChapterMeta {
    fn default() -> Self {
        Self {
            status: default_status(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub schema_version: u32,
    pub id: Uuid,
    pub title: String,
    pub blocks: Vec<Block>,
    pub meta: ChapterMeta,
}

impl Chapter {
    pub fn new(title: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: 1,
            id: Uuid::now_v7(),
            title: title.into(),
            blocks: vec![Block::new(
                super::BlockType::Narration,
                String::new(),
                now,
            )],
            meta: ChapterMeta::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn chapter_roundtrip_json() {
        let now = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let chapter = Chapter::new("第一章 开篇", now);
        let json = serde_json::to_string_pretty(&chapter).unwrap();
        assert!(json.contains(r#""schema_version": 1"#));
        assert!(json.contains(r#""status": "draft""#));

        let parsed: Chapter = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, chapter);
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.title, "第一章 开篇");
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.meta.status, "draft");
    }
}
