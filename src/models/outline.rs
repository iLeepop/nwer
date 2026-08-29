use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// 大纲分类（固定五种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutlineCategory {
    #[serde(rename = "角色")]
    Character,
    #[serde(rename = "背景")]
    Background,
    #[serde(rename = "场景")]
    Scene,
    #[serde(rename = "事件")]
    Event,
    #[serde(rename = "杂项")]
    Misc,
}

impl OutlineCategory {
    pub fn label(self) -> &'static str {
        match self {
            OutlineCategory::Character => "角色",
            OutlineCategory::Background => "背景",
            OutlineCategory::Scene => "场景",
            OutlineCategory::Event => "事件",
            OutlineCategory::Misc => "杂项",
        }
    }

    pub fn all() -> [OutlineCategory; 5] {
        [
            OutlineCategory::Character,
            OutlineCategory::Background,
            OutlineCategory::Scene,
            OutlineCategory::Event,
            OutlineCategory::Misc,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineMeta {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OutlineMeta {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineEntry {
    pub schema_version: u32,
    pub id: Uuid,
    pub key: String,
    pub category: OutlineCategory,
    pub fields: BTreeMap<String, String>,
    pub meta: OutlineMeta,
}

impl OutlineEntry {
    pub fn new(key: impl Into<String>, category: OutlineCategory, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: 1,
            id: Uuid::now_v7(),
            key: key.into(),
            category,
            fields: BTreeMap::new(),
            meta: OutlineMeta::new(now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn outline_entry_roundtrip_json() {
        let now = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let mut entry = OutlineEntry::new("张三", OutlineCategory::Character, now);
        entry.fields.insert("年龄".into(), "18".into());
        entry.fields.insert("身份".into(), "青云门外门弟子".into());
        entry.fields.insert("性格".into(), "沉稳内敛".into());
        entry.meta.updated_at = Utc.with_ymd_and_hms(2026, 8, 29, 8, 5, 0).unwrap();

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains(r#""category": "角色""#));
        assert!(json.contains(r#""schema_version": 1"#));

        let parsed: OutlineEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
        assert_eq!(parsed.category, OutlineCategory::Character);
        assert_eq!(parsed.fields.get("年龄").map(String::as_str), Some("18"));
    }

    #[test]
    fn outline_categories_serialize_as_chinese_labels() {
        for category in OutlineCategory::all() {
            let json = serde_json::to_string(&category).unwrap();
            assert_eq!(json, format!("\"{}\"", category.label()));
            let parsed: OutlineCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, category);
        }
    }
}
