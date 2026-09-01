use std::path::PathBuf;

use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::ai::{AiIntent, ProjectMutator, ScriptBlockUpdateFields};
use crate::models::{Block, BlockType, Chapter, OutlineCategory, Script};
use crate::storage::{
    create_chapter_file, create_directory, create_outline_entry, create_script_directory,
    create_script_file, delete_node, delete_outline_entry, delete_script_node,
    find_node_by_chapter_id, find_node_by_script_id, is_nonempty_directory, list_outline_entries,
    load_chapter, load_project, load_script, move_node, move_script_node, move_script_sibling,
    move_sibling, outline_entry_path, rename_node, rename_script_node, resolve_rel,
    resolve_script_rel, save_chapter, save_outline_entry, save_project, save_script,
    scan_chapter_tree, scan_script_tree, script_dir_nonempty, validate_storage_name, MoveDirection,
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

    fn apply_update_block(
        &mut self,
        chapter_id: Uuid,
        block_id: Uuid,
        content: Option<String>,
        block_type: Option<BlockType>,
        speaker: Option<String>,
    ) -> anyhow::Result<()> {
        let (path, mut chapter) = self.chapter_file(chapter_id)?;
        let idx = chapter
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("block {block_id} not found"))?;
        if let Some(content) = content {
            chapter.set_block_content(idx, content, self.now)?;
        }
        if let Some(block_type) = block_type {
            chapter.set_block_type(idx, block_type, self.now)?;
        }
        if let Some(speaker) = speaker {
            chapter.set_speaker(idx, Some(speaker), self.now)?;
        }
        save_chapter(&path, &chapter)
    }

    fn apply_delete_block(&mut self, chapter_id: Uuid, block_id: Uuid) -> anyhow::Result<()> {
        let (path, mut chapter) = self.chapter_file(chapter_id)?;
        let idx = chapter
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("block {block_id} not found"))?;
        chapter.remove_block(idx)?;
        save_chapter(&path, &chapter)
    }

    fn apply_move_block(
        &mut self,
        chapter_id: Uuid,
        block_id: Uuid,
        to_index: usize,
    ) -> anyhow::Result<()> {
        let (path, mut chapter) = self.chapter_file(chapter_id)?;
        let from = chapter
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("block {block_id} not found"))?;
        chapter.move_block(from, to_index)?;
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

    fn apply_delete_outline_entry(&mut self, id: Uuid) -> anyhow::Result<()> {
        let entries = list_outline_entries(&self.project_dir)?;
        let entry = entries
            .into_iter()
            .find(|e| e.id == id)
            .with_context(|| format!("outline entry {id} not found"))?;
        delete_outline_entry(&self.project_dir, entry.category, &entry.key)
    }

    fn apply_create_script(
        &mut self,
        title: String,
        parent_rel: String,
        script_id: Option<Uuid>,
        name: Option<String>,
    ) -> anyhow::Result<()> {
        let mut script = Script::new(title.clone(), self.now);
        if let Some(id) = script_id {
            script.id = id;
        }
        let stem = match name {
            Some(n) => {
                validate_storage_name(&n)?;
                n
            }
            None => script_file_stem(&title, script.id),
        };
        create_script_file(
            &self.project_dir,
            &parent_rel,
            &stem,
            &script,
            self.max_depth(),
        )?;
        Ok(())
    }

    fn apply_create_chapter_directory(
        &mut self,
        parent_rel: String,
        name: String,
    ) -> anyhow::Result<()> {
        create_directory(&self.project_dir, &parent_rel, &name, self.max_depth())?;
        Ok(())
    }

    fn apply_create_chapter_file(
        &mut self,
        chapter_id: Uuid,
        parent_rel: String,
        name: String,
        title: String,
    ) -> anyhow::Result<()> {
        let chapter = Chapter {
            schema_version: 1,
            id: chapter_id,
            title,
            blocks: vec![],
            meta: Default::default(),
        };
        create_chapter_file(
            &self.project_dir,
            &parent_rel,
            &name,
            &chapter,
            self.max_depth(),
        )?;
        Ok(())
    }

    fn apply_rename_chapter_node(
        &mut self,
        rel_path: String,
        new_name: String,
    ) -> anyhow::Result<()> {
        rename_node(&self.project_dir, &rel_path, &new_name)?;
        Ok(())
    }

    fn apply_delete_chapter_node(&mut self, rel_path: String) -> anyhow::Result<()> {
        if is_nonempty_directory(&self.project_dir, &rel_path)? {
            bail!("directory is not empty: {rel_path}");
        }
        delete_node(&self.project_dir, &rel_path)
    }

    fn apply_move_chapter_node(
        &mut self,
        rel_path: String,
        dest_parent_rel: String,
    ) -> anyhow::Result<()> {
        move_node(
            &self.project_dir,
            &rel_path,
            &dest_parent_rel,
            None,
            self.max_depth(),
        )?;
        Ok(())
    }

    fn apply_move_chapter_sibling(
        &mut self,
        rel_path: String,
        direction: i8,
    ) -> anyhow::Result<()> {
        let dir = match direction {
            d if d < 0 => MoveDirection::Up,
            _ => MoveDirection::Down,
        };
        move_sibling(&self.project_dir, &rel_path, dir)?;
        Ok(())
    }

    fn apply_copy_chapter(
        &mut self,
        src_rel: String,
        dest_parent_rel: String,
        new_name: String,
        new_chapter_id: Uuid,
    ) -> anyhow::Result<()> {
        let src = resolve_rel(&self.project_dir, &src_rel)?;
        let mut chapter = load_chapter(&src)?;
        chapter.id = new_chapter_id;
        create_chapter_file(
            &self.project_dir,
            &dest_parent_rel,
            &new_name,
            &chapter,
            self.max_depth(),
        )?;
        Ok(())
    }

    fn apply_update_chapter_title(
        &mut self,
        chapter_id: Uuid,
        title: String,
    ) -> anyhow::Result<()> {
        let (path, mut chapter) = self.chapter_file(chapter_id)?;
        chapter.title = title;
        save_chapter(&path, &chapter)
    }

    fn apply_create_script_directory(
        &mut self,
        parent_rel: String,
        name: String,
    ) -> anyhow::Result<()> {
        create_script_directory(&self.project_dir, &parent_rel, &name, self.max_depth())?;
        Ok(())
    }

    fn apply_rename_script_node(
        &mut self,
        rel_path: String,
        new_name: String,
    ) -> anyhow::Result<()> {
        rename_script_node(&self.project_dir, &rel_path, &new_name)?;
        Ok(())
    }

    fn apply_delete_script_node(&mut self, rel_path: String) -> anyhow::Result<()> {
        if script_dir_nonempty(&self.project_dir, &rel_path)? {
            bail!("directory is not empty: {rel_path}");
        }
        delete_script_node(&self.project_dir, &rel_path)
    }

    fn apply_move_script_node(
        &mut self,
        rel_path: String,
        dest_parent_rel: String,
    ) -> anyhow::Result<()> {
        move_script_node(
            &self.project_dir,
            &rel_path,
            &dest_parent_rel,
            None,
            self.max_depth(),
        )?;
        Ok(())
    }

    fn apply_move_script_sibling(
        &mut self,
        rel_path: String,
        direction: i8,
    ) -> anyhow::Result<()> {
        let dir = match direction {
            d if d < 0 => MoveDirection::Up,
            _ => MoveDirection::Down,
        };
        move_script_sibling(&self.project_dir, &rel_path, dir)?;
        Ok(())
    }

    fn apply_copy_script(
        &mut self,
        src_rel: String,
        dest_parent_rel: String,
        new_name: String,
        new_script_id: Uuid,
    ) -> anyhow::Result<()> {
        let src = resolve_script_rel(&self.project_dir, &src_rel)?;
        let mut script = load_script(&src)?;
        script.id = new_script_id;
        create_script_file(
            &self.project_dir,
            &dest_parent_rel,
            &new_name,
            &script,
            self.max_depth(),
        )?;
        Ok(())
    }

    fn apply_update_script_title(
        &mut self,
        script_id: Uuid,
        title: String,
    ) -> anyhow::Result<()> {
        let (path, mut script) = self.script_file(script_id)?;
        script.title = title;
        script.meta.updated_at = self.now;
        save_script(&path, &script)
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

    fn apply_delete_script_block(
        &mut self,
        script_id: Uuid,
        block_id: Uuid,
    ) -> anyhow::Result<()> {
        let (path, mut script) = self.script_file(script_id)?;
        let idx = script
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("script block {block_id} not found"))?;
        script.remove_block(idx)?;
        script.meta.updated_at = self.now;
        save_script(&path, &script)
    }

    fn apply_move_script_block(
        &mut self,
        script_id: Uuid,
        block_id: Uuid,
        to_index: usize,
    ) -> anyhow::Result<()> {
        let (path, mut script) = self.script_file(script_id)?;
        let from = script
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("script block {block_id} not found"))?;
        script.move_block(from, to_index)?;
        script.meta.updated_at = self.now;
        save_script(&path, &script)
    }

    fn apply_update_project_meta(
        &mut self,
        title: Option<String>,
        style_guide: Option<String>,
        synopsis: Option<String>,
    ) -> anyhow::Result<()> {
        let mut project = load_project(&self.project_dir)?;
        if let Some(t) = title {
            project.title = t;
        }
        if let Some(s) = style_guide {
            project.ai_context.style_guide = s;
        }
        if let Some(s) = synopsis {
            project.ai_context.synopsis = s;
        }
        project.updated_at = self.now;
        save_project(&self.project_dir, &project)
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
            AiIntent::UpdateBlock {
                chapter_id,
                block_id,
                content,
                block_type,
                speaker,
                ..
            } => self.apply_update_block(
                *chapter_id,
                *block_id,
                content.clone(),
                *block_type,
                speaker.clone(),
            ),
            AiIntent::DeleteBlock {
                chapter_id,
                block_id,
                ..
            } => self.apply_delete_block(*chapter_id, *block_id),
            AiIntent::MoveBlock {
                chapter_id,
                block_id,
                to_index,
                ..
            } => self.apply_move_block(*chapter_id, *block_id, *to_index),
            AiIntent::CreateChapterDirectory {
                parent_rel, name, ..
            } => self.apply_create_chapter_directory(parent_rel.clone(), name.clone()),
            AiIntent::CreateChapterFile {
                chapter_id,
                parent_rel,
                name,
                title,
                ..
            } => self.apply_create_chapter_file(
                *chapter_id,
                parent_rel.clone(),
                name.clone(),
                title.clone(),
            ),
            AiIntent::RenameChapterNode {
                rel_path, new_name, ..
            } => self.apply_rename_chapter_node(rel_path.clone(), new_name.clone()),
            AiIntent::DeleteChapterNode { rel_path, .. } => {
                self.apply_delete_chapter_node(rel_path.clone())
            }
            AiIntent::MoveChapterNode {
                rel_path,
                dest_parent_rel,
                ..
            } => self.apply_move_chapter_node(rel_path.clone(), dest_parent_rel.clone()),
            AiIntent::MoveChapterSibling {
                rel_path,
                direction,
                ..
            } => self.apply_move_chapter_sibling(rel_path.clone(), *direction),
            AiIntent::CopyChapter {
                src_rel,
                dest_parent_rel,
                new_name,
                new_chapter_id,
                ..
            } => self.apply_copy_chapter(
                src_rel.clone(),
                dest_parent_rel.clone(),
                new_name.clone(),
                *new_chapter_id,
            ),
            AiIntent::UpdateChapterTitle {
                chapter_id, title, ..
            } => self.apply_update_chapter_title(*chapter_id, title.clone()),
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
            AiIntent::DeleteOutlineEntry { id, .. } => self.apply_delete_outline_entry(*id),
            AiIntent::CreateScript {
                title,
                parent_rel,
                script_id,
                name,
                ..
            } => self.apply_create_script(
                title.clone(),
                parent_rel.clone(),
                *script_id,
                name.clone(),
            ),
            AiIntent::CreateScriptDirectory {
                parent_rel, name, ..
            } => self.apply_create_script_directory(parent_rel.clone(), name.clone()),
            AiIntent::RenameScriptNode {
                rel_path, new_name, ..
            } => self.apply_rename_script_node(rel_path.clone(), new_name.clone()),
            AiIntent::DeleteScriptNode { rel_path, .. } => {
                self.apply_delete_script_node(rel_path.clone())
            }
            AiIntent::MoveScriptNode {
                rel_path,
                dest_parent_rel,
                ..
            } => self.apply_move_script_node(rel_path.clone(), dest_parent_rel.clone()),
            AiIntent::MoveScriptSibling {
                rel_path,
                direction,
                ..
            } => self.apply_move_script_sibling(rel_path.clone(), *direction),
            AiIntent::CopyScript {
                src_rel,
                dest_parent_rel,
                new_name,
                new_script_id,
                ..
            } => self.apply_copy_script(
                src_rel.clone(),
                dest_parent_rel.clone(),
                new_name.clone(),
                *new_script_id,
            ),
            AiIntent::UpdateScriptTitle {
                script_id, title, ..
            } => self.apply_update_script_title(*script_id, title.clone()),
            AiIntent::AppendScriptBlocks {
                script_id, blocks, ..
            } => self.apply_append_script_blocks(*script_id, blocks.clone()),
            AiIntent::UpdateScriptBlock {
                script_id,
                block_id,
                fields,
                ..
            } => self.apply_update_script_block(*script_id, *block_id, fields),
            AiIntent::DeleteScriptBlock {
                script_id,
                block_id,
                ..
            } => self.apply_delete_script_block(*script_id, *block_id),
            AiIntent::MoveScriptBlock {
                script_id,
                block_id,
                to_index,
                ..
            } => self.apply_move_script_block(*script_id, *block_id, *to_index),
            AiIntent::UpdateProjectMeta {
                title,
                style_guide,
                synopsis,
                ..
            } => self.apply_update_project_meta(
                title.clone(),
                style_guide.clone(),
                synopsis.clone(),
            ),
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

    #[test]
    fn workspace_mutator_create_directory_and_chapter_file() {
        let dir = tempdir().unwrap();
        let (project_dir, _project) = create_project(dir.path(), "测试书", now()).unwrap();
        let mut m = WorkspaceMutator::open(&project_dir).with_now(now());

        m.apply(&AiIntent::CreateChapterDirectory {
            intent_id: Uuid::nil(),
            parent_rel: String::new(),
            name: "vol-001".into(),
        })
        .unwrap();

        let chapter_id = Uuid::from_u128(99);
        m.apply(&AiIntent::CreateChapterFile {
            intent_id: Uuid::nil(),
            chapter_id,
            parent_rel: "vol-001".into(),
            name: "ch-001".into(),
            title: "第一章".into(),
        })
        .unwrap();

        let tree = scan_chapter_tree(&project_dir).unwrap();
        let node = find_node_by_chapter_id(&tree, chapter_id).expect("chapter on disk");
        assert_eq!(node.rel_path, "vol-001/ch-001.json");
        let path = resolve_rel(&project_dir, &node.rel_path).unwrap();
        let loaded = load_chapter(&path).unwrap();
        assert_eq!(loaded.title, "第一章");
        assert_eq!(loaded.id, chapter_id);
    }

    #[test]
    fn workspace_apply_all_create_script_then_append_blocks() {
        use crate::ai::{apply_all, Proposal, ProposalStore};
        use crate::models::{ScriptBlock, ScriptBlockType};
        use crate::storage::{
            create_script_directory, find_node_by_script_id, load_script, resolve_script_rel,
            scan_script_tree,
        };

        let dir = tempdir().unwrap();
        let (project_dir, _project) = create_project(dir.path(), "测试书", now()).unwrap();
        create_script_directory(&project_dir, "", "vol-001", 3).unwrap();

        let script_id = Uuid::from_u128(501);
        let mut store = ProposalStore::default();
        store.push(Proposal {
            intent: AiIntent::CreateScript {
                intent_id: Uuid::from_u128(1),
                title: "晋升答辩".into(),
                parent_rel: "vol-001".into(),
                script_id: Some(script_id),
                name: Some("sc-001".into()),
            },
            stale: false,
        });
        store.push(Proposal {
            intent: AiIntent::AppendScriptBlocks {
                intent_id: Uuid::from_u128(2),
                script_id,
                blocks: vec![ScriptBlock::new(
                    ScriptBlockType::Action,
                    "王铁柱走上台。",
                    now(),
                )],
            },
            stale: false,
        });

        let mut m = WorkspaceMutator::open(&project_dir).with_now(now());
        apply_all(&mut store, &mut m).unwrap();
        assert!(store.is_empty());

        let tree = scan_script_tree(&project_dir).unwrap();
        let node = find_node_by_script_id(&tree, script_id).expect("script on disk");
        assert_eq!(node.rel_path, "vol-001/sc-001.json");
        let path = resolve_script_rel(&project_dir, &node.rel_path).unwrap();
        let loaded = load_script(&path).unwrap();
        assert_eq!(loaded.id, script_id);
        assert!(
            loaded.blocks.iter().any(|b| b.content == "王铁柱走上台。"),
            "expected appended block"
        );
    }
}
