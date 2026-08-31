use std::collections::HashMap;

use anyhow::Context;
use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use crate::ai::{AiIntent, ScriptBlockUpdateFields};
use crate::models::{
    Block, BlockType, Chapter, OutlineCategory, OutlineEntry, Script, ScriptBlock,
};

/// 将 AiIntent 写入项目存储的可插拔后端。
pub trait ProjectMutator {
    fn apply(&mut self, intent: &AiIntent) -> anyhow::Result<()>;
}

/// 内存中的项目变更后端，供 AI 写工具与测试使用。
#[derive(Debug, Clone)]
pub struct InMemoryMutator {
    pub title: String,
    pub style_guide: String,
    pub synopsis: String,
    chapters: HashMap<Uuid, Chapter>,
    scripts: HashMap<Uuid, Script>,
    outline: Vec<OutlineEntry>,
    now: DateTime<Utc>,
}

impl Default for InMemoryMutator {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryMutator {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            style_guide: String::new(),
            synopsis: String::new(),
            chapters: HashMap::new(),
            scripts: HashMap::new(),
            outline: Vec::new(),
            now: Utc::now(),
        }
    }

    pub fn with_now(now: DateTime<Utc>) -> Self {
        Self {
            now,
            ..Self::new()
        }
    }

    /// 带样例章节与项目元信息的 mutator，供读工具测试使用。
    pub fn with_sample_chapter() -> Self {
        use crate::ai::AiIntent;
        use crate::models::BlockType;

        let mut m = Self::with_now(Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap());
        m.title = "测试项目".into();
        m.style_guide = "简洁文风".into();
        m.synopsis = "样例简介".into();

        let ch_id = Uuid::from_u128(1);
        m.ensure_chapter(ch_id, "第一章");
        m.apply(&AiIntent::CreateBlock {
            intent_id: Uuid::nil(),
            chapter_id: ch_id,
            block_type: BlockType::Narration,
            content: "开篇".into(),
            speaker: None,
            after_block_id: None,
        })
        .expect("sample chapter block");

        m
    }

    pub fn list_chapters(&self) -> Vec<(Uuid, String)> {
        let mut items: Vec<_> = self
            .chapters
            .iter()
            .map(|(id, ch)| (*id, ch.title.clone()))
            .collect();
        items.sort_by_key(|(id, _)| *id);
        items
    }

    pub fn list_scripts(&self) -> Vec<(Uuid, String)> {
        let mut items: Vec<_> = self
            .scripts
            .iter()
            .map(|(id, s)| (*id, s.title.clone()))
            .collect();
        items.sort_by_key(|(id, _)| *id);
        items
    }

    /// 若章节不存在则创建空块列表的章节（不使用 `Chapter::new` 的默认块）。
    pub fn ensure_chapter(&mut self, id: Uuid, title: impl Into<String>) {
        self.chapters.entry(id).or_insert_with(|| Chapter {
            schema_version: 1,
            id,
            title: title.into(),
            blocks: vec![],
            meta: Default::default(),
        });
    }

    pub fn ensure_script(&mut self, script: Script) {
        self.scripts.insert(script.id, script);
    }

    /// 用完整章节覆盖/写入内存（读工具 hydrate）。
    pub fn upsert_chapter(&mut self, chapter: Chapter) {
        self.chapters.insert(chapter.id, chapter);
    }

    pub fn get_chapter(&self, id: Uuid) -> Option<&Chapter> {
        self.chapters.get(&id)
    }

    pub fn set_outline(&mut self, entries: Vec<OutlineEntry>) {
        self.outline = entries;
    }

    pub fn chapter_blocks(&self, id: Uuid) -> Option<&[Block]> {
        self.chapters.get(&id).map(|c| c.blocks.as_slice())
    }

    pub fn get_outline(&self) -> &[OutlineEntry] {
        &self.outline
    }

    pub fn get_script(&self, id: Uuid) -> Option<&Script> {
        self.scripts.get(&id)
    }

    pub fn list_chapter_ids(&self) -> Vec<Uuid> {
        let mut ids: Vec<_> = self.chapters.keys().copied().collect();
        ids.sort();
        ids
    }

    pub fn list_script_ids(&self) -> Vec<Uuid> {
        let mut ids: Vec<_> = self.scripts.keys().copied().collect();
        ids.sort();
        ids
    }

    fn chapter_mut(&mut self, id: Uuid) -> anyhow::Result<&mut Chapter> {
        self.chapters
            .get_mut(&id)
            .with_context(|| format!("chapter {id} not found"))
    }

    fn script_mut(&mut self, id: Uuid) -> anyhow::Result<&mut Script> {
        self.scripts
            .get_mut(&id)
            .with_context(|| format!("script {id} not found"))
    }

    fn apply_create_block(
        &mut self,
        chapter_id: Uuid,
        block_type: BlockType,
        content: String,
        speaker: Option<String>,
        after_block_id: Option<Uuid>,
    ) -> anyhow::Result<()> {
        let now = self.now;
        let mut block = Block::new(block_type, content, now);
        if let Some(s) = speaker {
            if block_type.allows_speaker() {
                block.speaker = Some(s);
            }
        }

        let chapter = self.chapter_mut(chapter_id)?;
        let insert_at = if let Some(after_id) = after_block_id {
            let idx = chapter
                .blocks
                .iter()
                .position(|b| b.id == after_id)
                .with_context(|| format!("after_block_id {after_id} not found"))?;
            idx + 1
        } else {
            chapter.blocks.len()
        };
        chapter.insert_block(insert_at, block)?;
        Ok(())
    }

    /// 替换策略：删除所有 target 块（自高索引向低索引），在最小原索引处插入新块。
    fn apply_replace_blocks(
        &mut self,
        chapter_id: Uuid,
        target_ids: &[Uuid],
        blocks: Vec<Block>,
    ) -> anyhow::Result<()> {
        let chapter = self.chapter_mut(chapter_id)?;

        let mut indices = Vec::with_capacity(target_ids.len());
        for target_id in target_ids {
            let idx = chapter
                .blocks
                .iter()
                .position(|b| b.id == *target_id)
                .with_context(|| format!("target block {target_id} not found"))?;
            indices.push(idx);
        }

        let insert_at = *indices.iter().min().unwrap();
        indices.sort_unstable_by(|a, b| b.cmp(a));
        for idx in indices {
            chapter.remove_block(idx)?;
        }
        for (offset, block) in blocks.into_iter().enumerate() {
            chapter.insert_block(insert_at + offset, block)?;
        }
        Ok(())
    }

    fn apply_create_outline_entry(
        &mut self,
        category: OutlineCategory,
        key: String,
        fields: Option<std::collections::BTreeMap<String, String>>,
    ) -> anyhow::Result<()> {
        let mut entry = OutlineEntry::new(key, category, self.now);
        if let Some(f) = fields {
            entry.fields = f;
        }
        self.outline.push(entry);
        Ok(())
    }

    fn apply_update_outline_entry(
        &mut self,
        id: Uuid,
        key: Option<String>,
        category: Option<OutlineCategory>,
        fields: Option<std::collections::BTreeMap<String, String>>,
    ) -> anyhow::Result<()> {
        let entry = self
            .outline
            .iter_mut()
            .find(|e| e.id == id)
            .with_context(|| format!("outline entry {id} not found"))?;
        if let Some(k) = key {
            entry.key = k;
        }
        if let Some(c) = category {
            entry.category = c;
        }
        if let Some(f) = fields {
            entry.fields = f;
        }
        entry.meta.updated_at = self.now;
        Ok(())
    }

    fn apply_create_script(&mut self, title: String) -> anyhow::Result<Uuid> {
        let script = Script::new(title, self.now);
        let id = script.id;
        self.scripts.insert(id, script);
        Ok(id)
    }

    fn apply_append_script_blocks(
        &mut self,
        script_id: Uuid,
        blocks: Vec<ScriptBlock>,
    ) -> anyhow::Result<()> {
        let now = self.now;
        let script = self.script_mut(script_id)?;
        for block in blocks {
            script.insert_block(script.blocks.len(), block)?;
        }
        script.meta.updated_at = now;
        Ok(())
    }

    fn apply_update_script_block(
        &mut self,
        script_id: Uuid,
        block_id: Uuid,
        fields: &ScriptBlockUpdateFields,
    ) -> anyhow::Result<()> {
        let now = self.now;
        let script = self.script_mut(script_id)?;
        let idx = script
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("script block {block_id} not found"))?;

        if let Some(content) = &fields.content {
            script.set_block_content(idx, content.clone(), now)?;
        }
        if let Some(character) = &fields.character {
            script.set_character(idx, Some(character.clone()), now)?;
        }
        if let Some(block_type) = fields.block_type {
            script.set_block_type(idx, block_type, now)?;
        }
        script.meta.updated_at = now;
        Ok(())
    }
}

impl ProjectMutator for InMemoryMutator {
    fn apply(&mut self, intent: &AiIntent) -> anyhow::Result<()> {
        match intent {
            AiIntent::CreateBlock {
                chapter_id,
                block_type,
                content,
                speaker,
                after_block_id,
                ..
            } => self.apply_create_block(
                *chapter_id,
                *block_type,
                content.clone(),
                speaker.clone(),
                *after_block_id,
            ),
            AiIntent::ReplaceBlocks {
                chapter_id,
                target_ids,
                blocks,
                ..
            } => self.apply_replace_blocks(*chapter_id, target_ids, blocks.clone()),
            AiIntent::CreateOutlineEntry {
                category,
                key,
                fields,
                ..
            } => self.apply_create_outline_entry(*category, key.clone(), fields.clone()),
            AiIntent::UpdateOutlineEntry {
                id,
                key,
                category,
                fields,
                ..
            } => self.apply_update_outline_entry(*id, key.clone(), *category, fields.clone()),
            AiIntent::CreateScript { title, .. } => {
                self.apply_create_script(title.clone())?;
                Ok(())
            }
            AiIntent::AppendScriptBlocks {
                script_id,
                blocks,
                ..
            } => self.apply_append_script_blocks(*script_id, blocks.clone()),
            AiIntent::UpdateScriptBlock {
                script_id,
                block_id,
                fields,
                ..
            } => self.apply_update_script_block(*script_id, *block_id, fields),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ScriptBlockType;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
    }

    fn chapter_id() -> Uuid {
        Uuid::from_u128(1)
    }

    #[test]
    fn apply_create_block_appends() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        let ch_id = chapter_id();
        mutator.ensure_chapter(ch_id, "第一章");

        mutator
            .apply(&AiIntent::CreateBlock {
                intent_id: Uuid::nil(),
                chapter_id: ch_id,
                block_type: BlockType::Narration,
                content: "开篇".into(),
                speaker: None,
                after_block_id: None,
            })
            .unwrap();

        let blocks = mutator.chapter_blocks(ch_id).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, BlockType::Narration);
        assert_eq!(blocks[0].content, "开篇");
    }

    #[test]
    fn apply_replace_detects_missing_target() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        let ch_id = chapter_id();
        mutator.ensure_chapter(ch_id, "第一章");

        let missing = Uuid::from_u128(999);
        let err = mutator
            .apply(&AiIntent::ReplaceBlocks {
                intent_id: Uuid::nil(),
                chapter_id: ch_id,
                target_ids: vec![missing],
                blocks: vec![Block::new(BlockType::Narration, "新", fixed_now())],
            })
            .unwrap_err();

        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn apply_outline_and_script() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());

        mutator
            .apply(&AiIntent::CreateOutlineEntry {
                intent_id: Uuid::nil(),
                category: OutlineCategory::Character,
                key: "主角".into(),
                fields: None,
            })
            .unwrap();

        assert!(mutator
            .get_outline()
            .iter()
            .any(|e| e.key == "主角"));

        mutator
            .apply(&AiIntent::CreateScript {
                intent_id: Uuid::nil(),
                title: "第一集".into(),
            })
            .unwrap();

        let script_id = mutator.list_script_ids()[0];
        let action = ScriptBlock::new(ScriptBlockType::Action, "开门", fixed_now());

        mutator
            .apply(&AiIntent::AppendScriptBlocks {
                intent_id: Uuid::nil(),
                script_id,
                blocks: vec![action],
            })
            .unwrap();

        let script = mutator.get_script(script_id).unwrap();
        assert!(script.blocks.len() >= 2);
        assert!(script
            .blocks
            .iter()
            .any(|b| b.content == "开门"));
    }
}
