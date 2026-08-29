use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::BlockType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub max_depth: u32,
    pub word_count_exclude_types: Vec<BlockType>,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            max_depth: 3,
            word_count_exclude_types: vec![BlockType::Note, BlockType::SceneBreak],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiState {
    #[serde(default)]
    pub expanded_nodes: Vec<String>,
    #[serde(default = "default_sidebar_tab")]
    pub sidebar_tab: String,
    #[serde(default)]
    pub ai_panel_open: bool,
}

fn default_sidebar_tab() -> String {
    "chapters".to_string()
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            expanded_nodes: Vec::new(),
            sidebar_tab: default_sidebar_tab(),
            ai_panel_open: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiContext {
    #[serde(default)]
    pub style_guide: String,
    #[serde(default)]
    pub synopsis: String,
}

impl Default for AiContext {
    fn default() -> Self {
        Self {
            style_guide: String::new(),
            synopsis: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub last_opened_chapter: Option<Uuid>,
    #[serde(default)]
    pub total_word_count: u64,
    pub settings: ProjectSettings,
    pub ui_state: UiState,
    pub ai_context: AiContext,
}

impl Project {
    pub fn new(title: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: 1,
            id: Uuid::now_v7(),
            title: title.into(),
            created_at: now,
            updated_at: now,
            last_opened_chapter: None,
            total_word_count: 0,
            settings: ProjectSettings::default(),
            ui_state: UiState::default(),
            ai_context: AiContext::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::str::FromStr;

    #[test]
    fn project_roundtrip_json() {
        let created = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let updated = Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap();
        let chapter_id =
            Uuid::from_str("0198f86d-55be-7000-8000-000000000002").unwrap();

        let mut project = Project::new("示例小说", created);
        project.id = Uuid::from_str("0198f86d-55be-7000-8000-000000000001").unwrap();
        project.updated_at = updated;
        project.last_opened_chapter = Some(chapter_id);
        project.total_word_count = 128_403;
        project.ui_state.expanded_nodes =
            vec!["vol-001第一卷".into(), "part-001上篇".into()];
        project.ai_context.style_guide = "第三人称有限视角，古风".into();
        project.ai_context.synopsis = "少年修仙成长史".into();

        let json = serde_json::to_string_pretty(&project).unwrap();
        assert!(json.contains(r#""schema_version": 1"#));
        assert!(json.contains(r#""max_depth": 3"#));
        assert!(json.contains(r#""note""#));
        assert!(json.contains(r#""scene_break""#));
        assert!(json.contains(r#""sidebar_tab": "chapters""#));

        let parsed: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, project);
        assert_eq!(parsed.settings.word_count_exclude_types, vec![
            BlockType::Note,
            BlockType::SceneBreak,
        ]);
    }

    #[test]
    fn project_defaults_match_spec() {
        let now = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let project = Project::new("未命名", now);
        assert_eq!(project.schema_version, 1);
        assert_eq!(project.settings.max_depth, 3);
        assert!(!project.ui_state.ai_panel_open);
        assert_eq!(project.ui_state.sidebar_tab, "chapters");
        assert_eq!(project.total_word_count, 0);
        assert!(project.last_opened_chapter.is_none());
    }
}
