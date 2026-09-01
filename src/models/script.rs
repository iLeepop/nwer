use anyhow::{Result, bail, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::BlockMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptBlockType {
    SceneHeading,
    Action,
    Character,
    Dialogue,
    Transition,
    Camera,
    Music,
    Sfx,
    Mood,
    Note,
}

impl ScriptBlockType {
    pub fn counts_toward_word_total(self) -> bool {
        matches!(
            self,
            ScriptBlockType::Action
                | ScriptBlockType::Dialogue
                | ScriptBlockType::Camera
                | ScriptBlockType::Music
                | ScriptBlockType::Sfx
                | ScriptBlockType::Mood
        )
    }

    pub fn counts_as_dialogue_line(self) -> bool {
        matches!(self, ScriptBlockType::Dialogue)
    }

    pub fn allows_character(self) -> bool {
        matches!(self, ScriptBlockType::Character | ScriptBlockType::Dialogue)
    }

    /// 音乐 / 氛围：覆盖剧本块区间。
    pub fn is_span_cue(self) -> bool {
        matches!(self, ScriptBlockType::Music | ScriptBlockType::Mood)
    }

    pub fn allows_ends_at(self) -> bool {
        self.is_span_cue()
    }

    pub fn label(self) -> &'static str {
        match self {
            ScriptBlockType::SceneHeading => "场景标题",
            ScriptBlockType::Action => "动作",
            ScriptBlockType::Character => "角色",
            ScriptBlockType::Dialogue => "对话",
            ScriptBlockType::Transition => "转场",
            ScriptBlockType::Camera => "镜头指导",
            ScriptBlockType::Music => "音乐",
            ScriptBlockType::Sfx => "音效",
            ScriptBlockType::Mood => "氛围/情绪",
            ScriptBlockType::Note => "备注",
        }
    }

    /// Enter 分割后新块的默认类型。
    pub fn split_successor(self) -> Self {
        match self {
            ScriptBlockType::SceneHeading => ScriptBlockType::Action,
            ScriptBlockType::Character => ScriptBlockType::Dialogue,
            ScriptBlockType::Transition => ScriptBlockType::SceneHeading,
            ScriptBlockType::Action
            | ScriptBlockType::Dialogue
            | ScriptBlockType::Camera
            | ScriptBlockType::Music
            | ScriptBlockType::Sfx
            | ScriptBlockType::Mood
            | ScriptBlockType::Note => self,
        }
    }

    pub fn all() -> [Self; 10] {
        [
            ScriptBlockType::SceneHeading,
            ScriptBlockType::Action,
            ScriptBlockType::Character,
            ScriptBlockType::Dialogue,
            ScriptBlockType::Transition,
            ScriptBlockType::Camera,
            ScriptBlockType::Music,
            ScriptBlockType::Sfx,
            ScriptBlockType::Mood,
            ScriptBlockType::Note,
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptBlock {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub block_type: ScriptBlockType,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    /// 仅 `music` / `mood`：结束块 id（含该块）；音效等类型恒为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<Uuid>,
    pub meta: BlockMeta,
}

impl ScriptBlock {
    pub fn new(block_type: ScriptBlockType, content: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::now_v7(),
            block_type,
            content: content.into(),
            character: None,
            ends_at: None,
            meta: BlockMeta::new(now),
        }
    }

    pub fn new_character(name: impl Into<String>, now: DateTime<Utc>) -> Self {
        let name = name.into();
        Self {
            id: Uuid::now_v7(),
            block_type: ScriptBlockType::Character,
            content: name.clone(),
            character: Some(name),
            ends_at: None,
            meta: BlockMeta::new(now),
        }
    }

    pub fn new_dialogue(
        content: impl Into<String>,
        character: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            block_type: ScriptBlockType::Dialogue,
            content: content.into(),
            character: Some(character.into()),
            ends_at: None,
            meta: BlockMeta::new(now),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScriptMeta {
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "draft".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub schema_version: u32,
    pub id: Uuid,
    pub title: String,
    pub blocks: Vec<ScriptBlock>,
    pub meta: ScriptMeta,
}

impl Script {
    pub fn new(title: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: 2,
            id: Uuid::now_v7(),
            title: title.into(),
            blocks: vec![ScriptBlock::new(
                ScriptBlockType::SceneHeading,
                String::new(),
                now,
            )],
            meta: ScriptMeta {
                created_at: now,
                updated_at: now,
                status: default_status(),
            },
        }
    }

    pub fn insert_block(&mut self, index: usize, block: ScriptBlock) -> Result<()> {
        ensure!(index <= self.blocks.len(), "insert index out of range");
        self.blocks.insert(index, block);
        self.sanitize_ends_at();
        Ok(())
    }

    pub fn remove_block(&mut self, index: usize) -> Result<ScriptBlock> {
        ensure!(index < self.blocks.len(), "remove index out of range");
        let removed = self.blocks.remove(index);
        self.clear_ends_at_refs(removed.id);
        self.sanitize_ends_at();
        Ok(removed)
    }

    pub fn set_block_type(
        &mut self,
        index: usize,
        new_type: ScriptBlockType,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        block.block_type = new_type;
        if !new_type.allows_character() {
            block.character = None;
        }
        if !new_type.allows_ends_at() {
            block.ends_at = None;
        }
        if new_type == ScriptBlockType::Character {
            let name = block.content.trim().to_string();
            block.character = if name.is_empty() {
                None
            } else {
                Some(name.clone())
            };
            block.content = name;
        }
        block.meta.updated_at = now;
        self.sanitize_ends_at();
        Ok(())
    }

    pub fn set_character(
        &mut self,
        index: usize,
        character: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        ensure!(
            block.block_type.allows_character(),
            "character only valid on character or dialogue blocks"
        );
        block.character = character.clone();
        if block.block_type == ScriptBlockType::Character {
            block.content = character.unwrap_or_default();
        }
        block.meta.updated_at = now;
        Ok(())
    }

    pub fn set_block_content(
        &mut self,
        index: usize,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        let content = content.into();
        block.content = content.clone();
        if block.block_type == ScriptBlockType::SceneHeading {
            block.content = content.to_uppercase();
        }
        if block.block_type == ScriptBlockType::Character {
            let name = content.trim().to_string();
            block.character = if name.is_empty() {
                None
            } else {
                Some(name.clone())
            };
            block.content = name;
        }
        block.meta.updated_at = now;
        Ok(())
    }

    pub fn split_block_at(
        &mut self,
        index: usize,
        byte_offset: usize,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        ensure!(
            byte_offset <= block.content.len() && block.content.is_char_boundary(byte_offset),
            "split offset must be on a char boundary"
        );
        let successor = block.block_type.split_successor();
        let character = block.character.clone();

        let block = self.blocks.get_mut(index).unwrap();
        let after = block.content[byte_offset..].to_string();
        block.content.truncate(byte_offset);
        if block.block_type == ScriptBlockType::Character {
            block.character = Some(block.content.clone());
        }
        block.meta.updated_at = now;

        let mut new_block = ScriptBlock::new(successor, after, now);
        if successor == ScriptBlockType::Dialogue {
            new_block.character = character;
        }
        self.blocks.insert(index + 1, new_block);
        Ok(())
    }

    pub fn merge_blocks(&mut self, start: usize, end: usize, now: DateTime<Utc>) -> Result<()> {
        ensure!(end < self.blocks.len(), "merge end out of range");
        ensure!(end > start, "merge requires at least two blocks");
        for i in start..=end {
            if self.blocks[i].block_type == ScriptBlockType::Note {
                continue;
            }
            if !matches!(
                self.blocks[i].block_type,
                ScriptBlockType::Action
                    | ScriptBlockType::Dialogue
                    | ScriptBlockType::Camera
                    | ScriptBlockType::Music
                    | ScriptBlockType::Sfx
                    | ScriptBlockType::Mood
            ) {
                bail!("only continuous text-like blocks can be merged");
            }
        }

        let first_type = self.blocks[start].block_type;
        let character = if first_type.allows_character() {
            self.blocks[start].character.clone()
        } else {
            None
        };
        let content = self.blocks[start..=end]
            .iter()
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let first = &mut self.blocks[start];
        first.block_type = first_type;
        first.content = content;
        first.character = character;
        first.meta.updated_at = now;
        self.blocks.drain(start + 1..=end);
        Ok(())
    }

    pub fn move_block(&mut self, from: usize, to: usize) -> Result<()> {
        ensure!(from < self.blocks.len(), "move from out of range");
        ensure!(to <= self.blocks.len(), "move to out of range");
        if from == to || from + 1 == to {
            return Ok(());
        }
        let block = self.blocks.remove(from);
        let insert_at = if to > from { to - 1 } else { to };
        self.blocks.insert(insert_at, block);
        self.sanitize_ends_at();
        Ok(())
    }

    pub fn swap_block(&mut self, index: usize, up: bool) -> Result<()> {
        let target = if up {
            ensure!(index > 0, "already at top");
            index - 1
        } else {
            ensure!(index + 1 < self.blocks.len(), "already at bottom");
            index + 1
        };
        self.blocks.swap(index, target);
        self.sanitize_ends_at();
        Ok(())
    }

    /// 解析 `music` / `mood` 块的有效终点下标（含）。非区间类型返回 `None`。
    pub fn resolved_span_end_index(&self, start: usize) -> Option<usize> {
        let block = self.blocks.get(start)?;
        if !block.block_type.is_span_cue() {
            return None;
        }
        if let Some(end_id) = block.ends_at {
            if let Some(j) = self.blocks.iter().position(|b| b.id == end_id) {
                if j > start {
                    return Some(j);
                }
            }
        }
        let ty = block.block_type;
        for j in (start + 1)..self.blocks.len() {
            let b = &self.blocks[j];
            if b.block_type == ty || b.block_type == ScriptBlockType::SceneHeading {
                return Some(j - 1);
            }
        }
        Some(self.blocks.len() - 1)
    }

    /// 设置区间结束块。`None` 清除显式结束并回退默认规则。
    pub fn set_ends_at(
        &mut self,
        index: usize,
        ends_at: Option<Uuid>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        ensure!(
            block.block_type.allows_ends_at(),
            "ends_at only valid on music or mood blocks"
        );

        if let Some(end_id) = ends_at {
            let j = self
                .blocks
                .iter()
                .position(|b| b.id == end_id)
                .ok_or_else(|| anyhow::anyhow!("ends_at target block not found"))?;
            ensure!(j > index, "ends_at must point to a block after the start");
            if self.span_would_overlap_same_type(index, j) {
                bail!("same-type span cues must not overlap");
            }
            let block = self.blocks.get_mut(index).unwrap();
            block.ends_at = Some(end_id);
            block.meta.updated_at = now;
        } else {
            let block = self.blocks.get_mut(index).unwrap();
            block.ends_at = None;
            block.meta.updated_at = now;
        }
        Ok(())
    }

    /// 默认结束边界的人类可读说明（未设 ends_at 时）。
    pub fn default_span_end_hint(&self, start: usize) -> Option<&'static str> {
        let block = self.blocks.get(start)?;
        if !block.block_type.is_span_cue() || block.ends_at.is_some() {
            return None;
        }
        let ty = block.block_type;
        for j in (start + 1)..self.blocks.len() {
            let b = &self.blocks[j];
            if b.block_type == ty {
                return Some(match ty {
                    ScriptBlockType::Music => "至下一段音乐",
                    ScriptBlockType::Mood => "至下一段氛围",
                    _ => "至同类型块",
                });
            }
            if b.block_type == ScriptBlockType::SceneHeading {
                return Some("至下一场景");
            }
        }
        Some("至剧本末尾")
    }

    fn span_would_overlap_same_type(&self, start: usize, end: usize) -> bool {
        let ty = self.blocks[start].block_type;
        for (k, other) in self.blocks.iter().enumerate() {
            if k == start || other.block_type != ty {
                continue;
            }
            let Some(other_end) = self.resolved_span_end_index(k) else {
                continue;
            };
            if start <= other_end && k <= end {
                return true;
            }
        }
        false
    }

    fn clear_ends_at_refs(&mut self, removed_id: Uuid) {
        for block in &mut self.blocks {
            if block.ends_at == Some(removed_id) {
                block.ends_at = None;
            }
        }
    }

    /// 清除颠倒的 ends_at，以及显式区间内夹入同类型块的无效引用。
    fn sanitize_ends_at(&mut self) {
        let snapshot: Vec<(usize, Option<Uuid>, ScriptBlockType)> = self
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (i, b.ends_at, b.block_type))
            .collect();

        for (i, ends_at, ty) in &snapshot {
            if !ty.allows_ends_at() {
                if self.blocks[*i].ends_at.is_some() {
                    self.blocks[*i].ends_at = None;
                }
                continue;
            }
            let Some(end_id) = ends_at else {
                continue;
            };
            let Some(j) = self.blocks.iter().position(|b| b.id == *end_id) else {
                self.blocks[*i].ends_at = None;
                continue;
            };
            if j <= *i {
                self.blocks[*i].ends_at = None;
                continue;
            }
            let has_same_inside = self.blocks[*i + 1..=j]
                .iter()
                .any(|b| b.block_type == *ty);
            if has_same_inside {
                self.blocks[*i].ends_at = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 1, 0, 0).unwrap()
    }

    #[test]
    fn script_roundtrip_json() {
        let script = Script::new("第一集", now());
        let json = serde_json::to_string_pretty(&script).unwrap();
        assert!(json.contains(r#""schema_version": 2"#));
        let parsed: Script = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, script);
    }

    #[test]
    fn block_type_labels_and_counts() {
        assert_eq!(ScriptBlockType::SceneHeading.label(), "场景标题");
        assert!(!ScriptBlockType::SceneHeading.counts_toward_word_total());
        assert!(ScriptBlockType::Action.counts_toward_word_total());
        assert!(ScriptBlockType::Dialogue.counts_as_dialogue_line());
        assert!(ScriptBlockType::Character.allows_character());
    }

    #[test]
    fn split_character_creates_dialogue() {
        let mut script = Script::new("测", now());
        script.blocks = vec![ScriptBlock::new_character("张三", now())];
        let offset = "张".len();
        script.split_block_at(0, offset, now()).unwrap();
        assert_eq!(script.blocks.len(), 2);
        assert_eq!(script.blocks[0].block_type, ScriptBlockType::Character);
        assert_eq!(script.blocks[1].block_type, ScriptBlockType::Dialogue);
        assert_eq!(script.blocks[1].character.as_deref(), Some("张三"));
        assert_eq!(script.blocks[1].content, "三");
    }

    #[test]
    fn scene_heading_uppercases_on_save() {
        let mut script = Script::new("测", now());
        script.set_block_content(0, "int. 客厅 - 夜", now()).unwrap();
        assert_eq!(script.blocks[0].content, "INT. 客厅 - 夜");
    }

    #[test]
    fn set_block_type_clears_character() {
        let mut script = Script::new("测", now());
        script.blocks = vec![ScriptBlock::new_dialogue("你好", "甲", now())];
        script
            .set_block_type(0, ScriptBlockType::Action, now())
            .unwrap();
        assert!(script.blocks[0].character.is_none());
    }

    #[test]
    fn music_default_end_stops_at_earlier_of_same_type_or_scene() {
        let mut script = Script::new("测", now());
        let m1 = ScriptBlock::new(ScriptBlockType::Music, "BGM1", now());
        let a1 = ScriptBlock::new(ScriptBlockType::Action, "动作1", now());
        let m2 = ScriptBlock::new(ScriptBlockType::Music, "BGM2", now());
        let a2 = ScriptBlock::new(ScriptBlockType::Action, "动作2", now());
        let scene = ScriptBlock::new(ScriptBlockType::SceneHeading, "INT. 二", now());
        script.blocks = vec![m1, a1, m2, a2, scene];
        assert_eq!(script.resolved_span_end_index(0), Some(1));
    }

    #[test]
    fn music_default_end_stops_at_scene_when_no_same_type() {
        let mut script = Script::new("测", now());
        let m1 = ScriptBlock::new(ScriptBlockType::Music, "BGM", now());
        let a1 = ScriptBlock::new(ScriptBlockType::Action, "a", now());
        let scene = ScriptBlock::new(ScriptBlockType::SceneHeading, "INT. 二", now());
        let a2 = ScriptBlock::new(ScriptBlockType::Action, "b", now());
        script.blocks = vec![m1, a1, scene, a2];
        assert_eq!(script.resolved_span_end_index(0), Some(1));
    }

    #[test]
    fn explicit_ends_at_wins() {
        let mut script = Script::new("测", now());
        let mut m1 = ScriptBlock::new(ScriptBlockType::Music, "BGM", now());
        let a1 = ScriptBlock::new(ScriptBlockType::Action, "a", now());
        let a2 = ScriptBlock::new(ScriptBlockType::Action, "b", now());
        let scene = ScriptBlock::new(ScriptBlockType::SceneHeading, "INT. 二", now());
        m1.ends_at = Some(a2.id);
        script.blocks = vec![m1, a1, a2, scene];
        assert_eq!(script.resolved_span_end_index(0), Some(2));
    }

    #[test]
    fn set_ends_at_rejects_same_type_overlap() {
        let mut script = Script::new("测", now());
        let m1 = ScriptBlock::new(ScriptBlockType::Music, "A", now());
        let a1 = ScriptBlock::new(ScriptBlockType::Action, "x", now());
        let m2 = ScriptBlock::new(ScriptBlockType::Music, "B", now());
        let a2 = ScriptBlock::new(ScriptBlockType::Action, "y", now());
        let a3 = ScriptBlock::new(ScriptBlockType::Action, "z", now());
        script.blocks = vec![m1, a1, m2, a2, a3];
        // m1 默认 [0,1]；显式拉到 a3 会盖住 m2 → 与 m2 区间相交
        let a3_id = script.blocks[4].id;
        assert!(script.set_ends_at(0, Some(a3_id), now()).is_err());
    }

    #[test]
    fn remove_block_clears_ends_at_refs() {
        let mut script = Script::new("测", now());
        let m1 = ScriptBlock::new(ScriptBlockType::Music, "A", now());
        let a1 = ScriptBlock::new(ScriptBlockType::Action, "x", now());
        let a2 = ScriptBlock::new(ScriptBlockType::Action, "y", now());
        script.blocks = vec![m1, a1, a2];
        let end_id = script.blocks[2].id;
        script.set_ends_at(0, Some(end_id), now()).unwrap();
        script.remove_block(2).unwrap();
        assert!(script.blocks[0].ends_at.is_none());
    }

    #[test]
    fn set_block_type_to_sfx_clears_ends_at() {
        let mut script = Script::new("测", now());
        let mut m = ScriptBlock::new(ScriptBlockType::Music, "A", now());
        let a = ScriptBlock::new(ScriptBlockType::Action, "x", now());
        m.ends_at = Some(a.id);
        script.blocks = vec![m, a];
        script
            .set_block_type(0, ScriptBlockType::Sfx, now())
            .unwrap();
        assert!(script.blocks[0].ends_at.is_none());
        assert_eq!(script.blocks[0].block_type, ScriptBlockType::Sfx);
    }

    #[test]
    fn old_json_without_ends_at_loads() {
        let json = r#"{
      "schema_version": 1,
      "id": "01900000-0000-7000-8000-000000000001",
      "title": "旧",
      "blocks": [{
        "id": "01900000-0000-7000-8000-000000000002",
        "type": "music",
        "content": "旧 BGM",
        "meta": {"created_at": "2026-08-31T00:00:00Z", "updated_at": "2026-08-31T00:00:00Z", "note": ""}
      }],
      "meta": {"created_at": "2026-08-31T00:00:00Z", "updated_at": "2026-08-31T00:00:00Z", "status": "draft"}
    }"#;
        let script: Script = serde_json::from_str(json).unwrap();
        assert_eq!(script.blocks[0].block_type, ScriptBlockType::Music);
        assert!(script.blocks[0].ends_at.is_none());
    }

    #[test]
    fn sfx_label_and_word_count() {
        assert_eq!(ScriptBlockType::Sfx.label(), "音效");
        assert_eq!(ScriptBlockType::Music.label(), "音乐");
        assert!(ScriptBlockType::Sfx.counts_toward_word_total());
        assert!(!ScriptBlockType::Sfx.is_span_cue());
        assert!(ScriptBlockType::Music.is_span_cue());
        assert_eq!(ScriptBlockType::all().len(), 10);
    }

    #[test]
    fn mood_may_overlap_music() {
        let mut script = Script::new("测", now());
        let music = ScriptBlock::new(ScriptBlockType::Music, "BGM", now());
        let mood = ScriptBlock::new(ScriptBlockType::Mood, "紧张", now());
        let a1 = ScriptBlock::new(ScriptBlockType::Action, "x", now());
        let a2 = ScriptBlock::new(ScriptBlockType::Action, "y", now());
        script.blocks = vec![music, mood, a1, a2];
        let end = script.blocks[3].id;
        script.set_ends_at(0, Some(end), now()).unwrap();
        script.set_ends_at(1, Some(end), now()).unwrap();
        assert_eq!(script.resolved_span_end_index(0), Some(3));
        assert_eq!(script.resolved_span_end_index(1), Some(3));
    }
}
