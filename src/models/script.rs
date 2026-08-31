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
                | ScriptBlockType::Mood
        )
    }

    pub fn counts_as_dialogue_line(self) -> bool {
        matches!(self, ScriptBlockType::Dialogue)
    }

    pub fn allows_character(self) -> bool {
        matches!(self, ScriptBlockType::Character | ScriptBlockType::Dialogue)
    }

    pub fn label(self) -> &'static str {
        match self {
            ScriptBlockType::SceneHeading => "场景标题",
            ScriptBlockType::Action => "动作",
            ScriptBlockType::Character => "角色",
            ScriptBlockType::Dialogue => "对话",
            ScriptBlockType::Transition => "转场",
            ScriptBlockType::Camera => "镜头指导",
            ScriptBlockType::Music => "音乐/音效",
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
            | ScriptBlockType::Mood
            | ScriptBlockType::Note => self,
        }
    }

    pub fn all() -> [Self; 9] {
        [
            ScriptBlockType::SceneHeading,
            ScriptBlockType::Action,
            ScriptBlockType::Character,
            ScriptBlockType::Dialogue,
            ScriptBlockType::Transition,
            ScriptBlockType::Camera,
            ScriptBlockType::Music,
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
    pub meta: BlockMeta,
}

impl ScriptBlock {
    pub fn new(block_type: ScriptBlockType, content: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::now_v7(),
            block_type,
            content: content.into(),
            character: None,
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
            schema_version: 1,
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
        Ok(())
    }

    pub fn remove_block(&mut self, index: usize) -> Result<ScriptBlock> {
        ensure!(index < self.blocks.len(), "remove index out of range");
        Ok(self.blocks.remove(index))
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
        Ok(())
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
        assert!(json.contains(r#""schema_version": 1"#));
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
}
