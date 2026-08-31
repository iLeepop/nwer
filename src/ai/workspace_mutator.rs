use std::path::PathBuf;

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ai::{AiIntent, ProjectMutator, ScriptBlockUpdateFields};
use crate::models::{Block, BlockType, Chapter, OutlineCategory, Script};
use crate::storage::{
    create_outline_entry, create_script_file, delete_outline_entry, find_node_by_chapter_id,
    find_node_by_script_id, list_outline_entries, load_chapter, load_project, load_script,
    outline_entry_path, resolve_rel, resolve_script_rel, save_chapter, save_outline_entry,
    save_script, scan_chapter_tree, scan_script_tree, validate_storage_name,
};

/// 将 AiIntent 写入真实项目目录。
pub struct WorkspaceMutator {
    pub project_dir: PathBuf,
    now: DateTime<Utc>,
}

impl WorkspaceMutator {
    pub fn open(project_dir: impl Into<PathBuf>) -> Self {
        Self {
            project_dir: project_dir.into(),
            now: Utc::now(),
        }
    }

    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.now = now;
        self
    }

    fn max_depth(&self) -> u32 {
        load_project(&self.project_dir)
            .map(|p| p.settings.max_depth.max(1))
            .unwrap_or(3)
    }

    fn chapter_file(&self, chapter_id: Uuid) -> anyhow::Result<(PathBuf, Chapter)> {
        let tree = scan_chapter_tree(&self.project_dir)?;
        let node = find_node_by_chapter_id(&tree, chapter_id)
            .with_context(|| format!("chapter {chapter_id} not found"))?;
        let path = resolve_rel(&self.project_dir, &node.rel_path)?;
        let chapter = load_chapter(&path)?;
        Ok((path, chapter))
    }

    fn script_file(&self, script_id: Uuid) -> anyhow::Result<(PathBuf, Script)> {
        let tree = scan_script_tree(&self.project_dir)?;
        let node = find_node_by_script_id(&tree, script_id)
            .with_context(|| format!("script {script_id} not found"))?;
        let path = resolve_script_rel(&self.project_dir, &node.rel_path)?;
        let script = load_script(&path)?;
        Ok((path, script))
    }

    fn apply_create_block(
        &mut self,
        chapter_id: Uuid,
        block_type: BlockType,
        content: String,
        speaker: Option<String>,
        after_block_id: Option<Uuid>,
    ) -> anyhow::Result<()> {
        let (path, mut chapter) = self.chapter_file(chapter_id)?;
        let mut block = Block::new(block_type, content, self.now);
        if let Some(s) = speaker {
            if block_type.allows_speaker() {
                block.speaker = Some(s);
            }
        }
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
        save_chapter(&path, &chapter)
    }

    fn apply_replace_blocks(
        &mut self,
        chapter_id: Uuid,
        target_ids: &[Uuid],
        blocks: Vec<Block>,
    ) -> anyhow::Result<()> {
        let (path, mut chapter) = self.chapter_file(chapter_id)?;
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
        save_chapter(&path, &chapter)
    }

    fn apply_create_outline_entry(
        &mut self,
        category: OutlineCategory,
        key: String,
        fields: Option<std::collections::BTreeMap<String, String>>,
    ) -> anyhow::Result<()> {
        let mut entry = create_outline_entry(&self.project_dir, &key, category, self.now)?;
        if let Some(f) = fields {
            entry.fields = f;
            let path = outline_entry_path(&self.project_dir, entry.category, &entry.key)?;
            save_outline_entry(&path, &entry)?;
        }
        Ok(())
    }

    fn apply_update_outline_entry(
        &mut self,
        id: Uuid,
        key: Option<String>,
        category: Option<OutlineCategory>,
        fields: Option<std::collections::BTreeMap<String, String>>,
    ) -> anyhow::Result<()> {
        let entries = list_outline_entries(&self.project_dir)?;
        let mut entry = entries
            .into_iter()
            .find(|e| e.id == id)
            .with_context(|| format!("outline entry {id} not found"))?;
        let old_category = entry.category;
        let old_key = entry.key.clone();
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

        let moved = old_key != entry.key || old_category != entry.category;
        let new_path = outline_entry_path(&self.project_dir, entry.category, &entry.key)?;
        if moved && new_path.exists() {
            bail!("outline entry already exists: {}", new_path.display());
        }
        save_outline_entry(&new_path, &entry)?;
        if moved {
            delete_outline_entry(&self.project_dir, old_category, &old_key)?;
        }
        Ok(())
    }

    fn apply_create_script(&mut self, title: String) -> anyhow::Result<()> {
        let script = Script::new(title.clone(), self.now);
        let name = script_file_stem(&title, script.id);
        create_script_file(&self.project_dir, "", &name, &script, self.max_depth())?;
        Ok(())
    }

    fn apply_append_script_blocks(
        &mut self,
        script_id: Uuid,
        blocks: Vec<crate::models::ScriptBlock>,
    ) -> anyhow::Result<()> {
        let (path, mut script) = self.script_file(script_id)?;
        for block in blocks {
            script.insert_block(script.blocks.len(), block)?;
        }
        script.meta.updated_at = self.now;
        save_script(&path, &script)
    }

    fn apply_update_script_block(
        &mut self,
        script_id: Uuid,
        block_id: Uuid,
        fields: &ScriptBlockUpdateFields,
    ) -> anyhow::Result<()> {
        let (path, mut script) = self.script_file(script_id)?;
        let idx = script
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("script block {block_id} not found"))?;
        if let Some(content) = &fields.content {
            script.set_block_content(idx, content.clone(), self.now)?;
        }
        if let Some(character) = &fields.character {
            script.set_character(idx, Some(character.clone()), self.now)?;
        }
        if let Some(block_type) = fields.block_type {
            script.set_block_type(idx, block_type, self.now)?;
        }
        script.meta.updated_at = self.now;
        save_script(&path, &script)
    }
}

fn script_file_stem(title: &str, id: Uuid) -> String {
    if validate_storage_name(title).is_ok() {
        title.to_string()
    } else {
        format!("script-{id}")
    }
}

impl ProjectMutator for WorkspaceMutator {
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
            AiIntent::CreateScript { title, .. } => self.apply_create_script(title.clone()),
            AiIntent::AppendScriptBlocks {
                script_id, blocks, ..
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
    use crate::models::Chapter;
    use crate::storage::{create_chapter_file, create_project, load_chapter};
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
    }

    #[test]
    fn workspace_mutator_create_block_persists() {
        let dir = tempdir().unwrap();
        let (project_dir, _project) = create_project(dir.path(), "测试书", now()).unwrap();

        let chapter = Chapter::new("第一章", now());
        let chapter_id = chapter.id;
        create_chapter_file(&project_dir, "", "ch-001", &chapter, 3).unwrap();

        let mut m = WorkspaceMutator::open(&project_dir).with_now(now());
        m.apply(&AiIntent::CreateBlock {
            intent_id: Uuid::nil(),
            chapter_id,
            block_type: BlockType::Narration,
            content: "落盘正文".into(),
            speaker: None,
            after_block_id: None,
        })
        .unwrap();

        let tree = scan_chapter_tree(&project_dir).unwrap();
        let node = find_node_by_chapter_id(&tree, chapter_id).expect("chapter on disk");
        let path = resolve_rel(&project_dir, &node.rel_path).unwrap();
        let loaded = load_chapter(&path).unwrap();
        assert!(
            loaded.blocks.iter().any(|b| b.content == "落盘正文"),
            "expected persisted block, got {:?}",
            loaded.blocks.iter().map(|b| &b.content).collect::<Vec<_>>()
        );
    }
}
