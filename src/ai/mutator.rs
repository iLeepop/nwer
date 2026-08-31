use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context};
use chrono::{DateTime, TimeZone, Utc};
use uuid::Uuid;

use crate::ai::{AiIntent, ScriptBlockUpdateFields};
use crate::models::{
    Block, BlockType, Chapter, OutlineCategory, OutlineEntry, Script, ScriptBlock,
};
use crate::storage::validate_storage_name;

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
    /// 相对 `chapters/` 的目录路径（不含尾斜杠）。
    dirs: HashSet<String>,
    /// 章节相对路径（含 `.json`）。
    chapter_rels: HashMap<Uuid, String>,
    scripts: HashMap<Uuid, Script>,
    /// 相对 `scripts/` 的目录路径（不含尾斜杠）。
    script_dirs: HashSet<String>,
    /// 剧本相对路径（含 `.json`）。
    script_rels: HashMap<Uuid, String>,
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
            dirs: HashSet::new(),
            chapter_rels: HashMap::new(),
            scripts: HashMap::new(),
            script_dirs: HashSet::new(),
            script_rels: HashMap::new(),
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

    /// hydrate：登记目录相对路径。
    pub fn ensure_dir(&mut self, rel: impl Into<String>) {
        let rel = rel.into();
        if !rel.is_empty() {
            self.dirs.insert(rel);
        }
    }

    /// hydrate：登记章节相对路径。
    pub fn set_chapter_rel(&mut self, id: Uuid, rel: impl Into<String>) {
        self.chapter_rels.insert(id, rel.into());
    }

    pub fn chapter_rel(&self, id: Uuid) -> Option<&str> {
        self.chapter_rels.get(&id).map(|s| s.as_str())
    }

    pub fn list_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<_> = self.dirs.iter().cloned().collect();
        dirs.sort();
        dirs
    }

    /// hydrate：登记剧本目录相对路径。
    pub fn ensure_script_dir(&mut self, rel: impl Into<String>) {
        let rel = rel.into();
        if !rel.is_empty() {
            self.script_dirs.insert(rel);
        }
    }

    /// hydrate：登记剧本相对路径。
    pub fn set_script_rel(&mut self, id: Uuid, rel: impl Into<String>) {
        self.script_rels.insert(id, rel.into());
    }

    pub fn script_rel(&self, id: Uuid) -> Option<&str> {
        self.script_rels.get(&id).map(|s| s.as_str())
    }

    pub fn list_script_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<_> = self.script_dirs.iter().cloned().collect();
        dirs.sort();
        dirs
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

    fn memory_rel_join(parent: &str, name: &str) -> String {
        if parent.is_empty() {
            name.to_string()
        } else {
            format!("{parent}/{name}")
        }
    }

    fn parent_rel(rel: &str) -> &str {
        rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
    }

    fn base_name(rel: &str) -> &str {
        rel.rsplit('/').next().unwrap_or(rel)
    }

    fn rewrite_prefix(path: &str, old: &str, new: &str) -> String {
        if path == old {
            new.to_string()
        } else if let Some(rest) = path.strip_prefix(&format!("{old}/")) {
            format!("{new}/{rest}")
        } else {
            path.to_string()
        }
    }

    fn ensure_parent_dir(&self, parent_rel: &str) -> anyhow::Result<()> {
        if parent_rel.is_empty() {
            return Ok(());
        }
        if !self.dirs.contains(parent_rel) {
            bail!("parent directory not found: {parent_rel}");
        }
        Ok(())
    }

    fn ensure_parent_script_dir(&self, parent_rel: &str) -> anyhow::Result<()> {
        if parent_rel.is_empty() {
            return Ok(());
        }
        if !self.script_dirs.contains(parent_rel) {
            bail!("parent directory not found: {parent_rel}");
        }
        Ok(())
    }

    fn chapter_dir_is_nonempty(&self, rel: &str) -> bool {
        let prefix = format!("{rel}/");
        self.dirs.iter().any(|d| d.starts_with(&prefix))
            || self.chapter_rels.values().any(|r| r.starts_with(&prefix))
    }

    fn script_dir_is_nonempty(&self, rel: &str) -> bool {
        let prefix = format!("{rel}/");
        self.script_dirs.iter().any(|d| d.starts_with(&prefix))
            || self.script_rels.values().any(|r| r.starts_with(&prefix))
    }

    fn find_chapter_id_by_rel(&self, rel: &str) -> anyhow::Result<Uuid> {
        self.chapter_rels
            .iter()
            .find(|(_, r)| r.as_str() == rel)
            .map(|(id, _)| *id)
            .with_context(|| format!("chapter node not found: {rel}"))
    }

    fn find_script_id_by_rel(&self, rel: &str) -> anyhow::Result<Uuid> {
        self.script_rels
            .iter()
            .find(|(_, r)| r.as_str() == rel)
            .map(|(id, _)| *id)
            .with_context(|| format!("script node not found: {rel}"))
    }

    fn script_file_stem(title: &str, id: Uuid) -> String {
        if validate_storage_name(title).is_ok() {
            title.to_string()
        } else {
            format!("script-{id}")
        }
    }

    fn apply_create_chapter_directory(
        &mut self,
        parent_rel: String,
        name: String,
    ) -> anyhow::Result<()> {
        validate_storage_name(&name)?;
        self.ensure_parent_dir(&parent_rel)?;
        let rel = Self::memory_rel_join(&parent_rel, &name);
        if self.dirs.contains(&rel) {
            bail!("directory already exists: {rel}");
        }
        self.dirs.insert(rel);
        Ok(())
    }

    fn apply_create_chapter_file(
        &mut self,
        chapter_id: Uuid,
        parent_rel: String,
        name: String,
        title: String,
    ) -> anyhow::Result<()> {
        validate_storage_name(&name)?;
        self.ensure_parent_dir(&parent_rel)?;
        if self.chapters.contains_key(&chapter_id) {
            bail!("chapter already exists: {chapter_id}");
        }
        let file_rel = Self::memory_rel_join(&parent_rel, &format!("{name}.json"));
        if self.chapter_rels.values().any(|r| r == &file_rel) {
            bail!("chapter file already exists: {file_rel}");
        }
        self.chapters.insert(
            chapter_id,
            Chapter {
                schema_version: 1,
                id: chapter_id,
                title,
                blocks: vec![],
                meta: Default::default(),
            },
        );
        self.chapter_rels.insert(chapter_id, file_rel);
        Ok(())
    }

    fn apply_rename_chapter_node(
        &mut self,
        rel_path: String,
        new_name: String,
    ) -> anyhow::Result<()> {
        if rel_path.is_empty() {
            bail!("cannot rename chapters root");
        }
        validate_storage_name(&new_name)?;
        let parent = Self::parent_rel(&rel_path);
        if self.dirs.contains(&rel_path) {
            let new_rel = Self::memory_rel_join(parent, &new_name);
            if self.dirs.contains(&new_rel) || self.chapter_rels.values().any(|r| r == &new_rel) {
                bail!("target already exists: {new_rel}");
            }
            self.dirs.remove(&rel_path);
            self.dirs.insert(new_rel.clone());
            let old_dirs: Vec<_> = self.dirs.iter().cloned().collect();
            for d in old_dirs {
                let rewritten = Self::rewrite_prefix(&d, &rel_path, &new_rel);
                if rewritten != d {
                    self.dirs.remove(&d);
                    self.dirs.insert(rewritten);
                }
            }
            for rel in self.chapter_rels.values_mut() {
                *rel = Self::rewrite_prefix(rel, &rel_path, &new_rel);
            }
            return Ok(());
        }
        let chapter_id = self.find_chapter_id_by_rel(&rel_path)?;
        let new_rel = Self::memory_rel_join(parent, &format!("{new_name}.json"));
        if self.chapter_rels.values().any(|r| r == &new_rel) || self.dirs.contains(&new_rel) {
            bail!("target already exists: {new_rel}");
        }
        self.chapter_rels.insert(chapter_id, new_rel);
        Ok(())
    }

    fn apply_delete_chapter_node(&mut self, rel_path: String) -> anyhow::Result<()> {
        if rel_path.is_empty() {
            bail!("cannot delete chapters root");
        }
        if self.dirs.contains(&rel_path) {
            if self.chapter_dir_is_nonempty(&rel_path) {
                bail!("directory is not empty: {rel_path}");
            }
            self.dirs.remove(&rel_path);
            return Ok(());
        }
        let chapter_id = self.find_chapter_id_by_rel(&rel_path)?;
        self.chapters.remove(&chapter_id);
        self.chapter_rels.remove(&chapter_id);
        Ok(())
    }

    fn apply_move_chapter_node(
        &mut self,
        rel_path: String,
        dest_parent_rel: String,
    ) -> anyhow::Result<()> {
        if rel_path.is_empty() {
            bail!("cannot move chapters root");
        }
        self.ensure_parent_dir(&dest_parent_rel)?;
        let base = Self::base_name(&rel_path).to_string();
        let new_rel = Self::memory_rel_join(&dest_parent_rel, &base);
        if new_rel == rel_path {
            return Ok(());
        }
        if self.dirs.contains(&new_rel) || self.chapter_rels.values().any(|r| r == &new_rel) {
            bail!("destination already exists: {new_rel}");
        }
        if self.dirs.contains(&rel_path) {
            if dest_parent_rel == rel_path
                || dest_parent_rel.starts_with(&format!("{rel_path}/"))
            {
                bail!("cannot move directory into itself or descendant");
            }
            self.dirs.remove(&rel_path);
            self.dirs.insert(new_rel.clone());
            let old_dirs: Vec<_> = self.dirs.iter().cloned().collect();
            for d in old_dirs {
                let rewritten = Self::rewrite_prefix(&d, &rel_path, &new_rel);
                if rewritten != d {
                    self.dirs.remove(&d);
                    self.dirs.insert(rewritten);
                }
            }
            for rel in self.chapter_rels.values_mut() {
                *rel = Self::rewrite_prefix(rel, &rel_path, &new_rel);
            }
            return Ok(());
        }
        let chapter_id = self.find_chapter_id_by_rel(&rel_path)?;
        self.chapter_rels.insert(chapter_id, new_rel);
        Ok(())
    }

    fn apply_move_chapter_sibling(
        &mut self,
        _rel_path: String,
        _direction: i8,
    ) -> anyhow::Result<()> {
        // 内存后端无磁盘兄弟交换语义；同目录重排为 best-effort noop。
        Ok(())
    }

    fn apply_copy_chapter(
        &mut self,
        src_rel: String,
        dest_parent_rel: String,
        new_name: String,
        new_chapter_id: Uuid,
    ) -> anyhow::Result<()> {
        validate_storage_name(&new_name)?;
        self.ensure_parent_dir(&dest_parent_rel)?;
        let src_id = self.find_chapter_id_by_rel(&src_rel)?;
        let src = self
            .chapters
            .get(&src_id)
            .with_context(|| format!("chapter {src_id} not found"))?
            .clone();
        if self.chapters.contains_key(&new_chapter_id) {
            bail!("chapter already exists: {new_chapter_id}");
        }
        let file_rel = Self::memory_rel_join(&dest_parent_rel, &format!("{new_name}.json"));
        if self.chapter_rels.values().any(|r| r == &file_rel) {
            bail!("chapter file already exists: {file_rel}");
        }
        self.chapters.insert(
            new_chapter_id,
            Chapter {
                schema_version: src.schema_version,
                id: new_chapter_id,
                title: src.title,
                blocks: src.blocks,
                meta: src.meta,
            },
        );
        self.chapter_rels.insert(new_chapter_id, file_rel);
        Ok(())
    }

    fn apply_update_chapter_title(
        &mut self,
        chapter_id: Uuid,
        title: String,
    ) -> anyhow::Result<()> {
        let chapter = self.chapter_mut(chapter_id)?;
        chapter.title = title;
        Ok(())
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

    fn apply_update_block(
        &mut self,
        chapter_id: Uuid,
        block_id: Uuid,
        content: Option<String>,
        block_type: Option<BlockType>,
        speaker: Option<String>,
    ) -> anyhow::Result<()> {
        let now = self.now;
        let chapter = self.chapter_mut(chapter_id)?;
        let idx = chapter
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("block {block_id} not found"))?;
        if let Some(content) = content {
            chapter.set_block_content(idx, content, now)?;
        }
        if let Some(block_type) = block_type {
            chapter.set_block_type(idx, block_type, now)?;
        }
        if let Some(speaker) = speaker {
            chapter.set_speaker(idx, Some(speaker), now)?;
        }
        Ok(())
    }

    fn apply_delete_block(&mut self, chapter_id: Uuid, block_id: Uuid) -> anyhow::Result<()> {
        let chapter = self.chapter_mut(chapter_id)?;
        let idx = chapter
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("block {block_id} not found"))?;
        chapter.remove_block(idx)?;
        Ok(())
    }

    fn apply_move_block(
        &mut self,
        chapter_id: Uuid,
        block_id: Uuid,
        to_index: usize,
    ) -> anyhow::Result<()> {
        let chapter = self.chapter_mut(chapter_id)?;
        let from = chapter
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("block {block_id} not found"))?;
        chapter.move_block(from, to_index)?;
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

    fn apply_delete_outline_entry(&mut self, id: Uuid) -> anyhow::Result<()> {
        let idx = self
            .outline
            .iter()
            .position(|e| e.id == id)
            .with_context(|| format!("outline entry {id} not found"))?;
        self.outline.remove(idx);
        Ok(())
    }

    fn apply_create_script(
        &mut self,
        title: String,
        parent_rel: String,
        script_id: Option<Uuid>,
        name: Option<String>,
    ) -> anyhow::Result<Uuid> {
        self.ensure_parent_script_dir(&parent_rel)?;
        let id = script_id.unwrap_or_else(Uuid::now_v7);
        if self.scripts.contains_key(&id) {
            bail!("script already exists: {id}");
        }
        let stem = match name {
            Some(n) => {
                validate_storage_name(&n)?;
                n
            }
            None => Self::script_file_stem(&title, id),
        };
        let file_rel = Self::memory_rel_join(&parent_rel, &format!("{stem}.json"));
        if self.script_rels.values().any(|r| r == &file_rel) {
            bail!("script file already exists: {file_rel}");
        }
        let mut script = Script::new(title, self.now);
        script.id = id;
        self.scripts.insert(id, script);
        self.script_rels.insert(id, file_rel);
        Ok(id)
    }

    fn apply_create_script_directory(
        &mut self,
        parent_rel: String,
        name: String,
    ) -> anyhow::Result<()> {
        validate_storage_name(&name)?;
        self.ensure_parent_script_dir(&parent_rel)?;
        let rel = Self::memory_rel_join(&parent_rel, &name);
        if self.script_dirs.contains(&rel) {
            bail!("directory already exists: {rel}");
        }
        self.script_dirs.insert(rel);
        Ok(())
    }

    fn apply_rename_script_node(
        &mut self,
        rel_path: String,
        new_name: String,
    ) -> anyhow::Result<()> {
        if rel_path.is_empty() {
            bail!("cannot rename scripts root");
        }
        validate_storage_name(&new_name)?;
        let parent = Self::parent_rel(&rel_path);
        if self.script_dirs.contains(&rel_path) {
            let new_rel = Self::memory_rel_join(parent, &new_name);
            if self.script_dirs.contains(&new_rel)
                || self.script_rels.values().any(|r| r == &new_rel)
            {
                bail!("target already exists: {new_rel}");
            }
            self.script_dirs.remove(&rel_path);
            self.script_dirs.insert(new_rel.clone());
            let old_dirs: Vec<_> = self.script_dirs.iter().cloned().collect();
            for d in old_dirs {
                let rewritten = Self::rewrite_prefix(&d, &rel_path, &new_rel);
                if rewritten != d {
                    self.script_dirs.remove(&d);
                    self.script_dirs.insert(rewritten);
                }
            }
            for rel in self.script_rels.values_mut() {
                *rel = Self::rewrite_prefix(rel, &rel_path, &new_rel);
            }
            return Ok(());
        }
        let script_id = self.find_script_id_by_rel(&rel_path)?;
        let new_rel = Self::memory_rel_join(parent, &format!("{new_name}.json"));
        if self.script_rels.values().any(|r| r == &new_rel) || self.script_dirs.contains(&new_rel)
        {
            bail!("target already exists: {new_rel}");
        }
        self.script_rels.insert(script_id, new_rel);
        Ok(())
    }

    fn apply_delete_script_node(&mut self, rel_path: String) -> anyhow::Result<()> {
        if rel_path.is_empty() {
            bail!("cannot delete scripts root");
        }
        if self.script_dirs.contains(&rel_path) {
            if self.script_dir_is_nonempty(&rel_path) {
                bail!("directory is not empty: {rel_path}");
            }
            self.script_dirs.remove(&rel_path);
            return Ok(());
        }
        let script_id = self.find_script_id_by_rel(&rel_path)?;
        self.scripts.remove(&script_id);
        self.script_rels.remove(&script_id);
        Ok(())
    }

    fn apply_move_script_node(
        &mut self,
        rel_path: String,
        dest_parent_rel: String,
    ) -> anyhow::Result<()> {
        if rel_path.is_empty() {
            bail!("cannot move scripts root");
        }
        self.ensure_parent_script_dir(&dest_parent_rel)?;
        let base = Self::base_name(&rel_path).to_string();
        let new_rel = Self::memory_rel_join(&dest_parent_rel, &base);
        if new_rel == rel_path {
            return Ok(());
        }
        if self.script_dirs.contains(&new_rel) || self.script_rels.values().any(|r| r == &new_rel)
        {
            bail!("destination already exists: {new_rel}");
        }
        if self.script_dirs.contains(&rel_path) {
            if dest_parent_rel == rel_path
                || dest_parent_rel.starts_with(&format!("{rel_path}/"))
            {
                bail!("cannot move directory into itself or descendant");
            }
            self.script_dirs.remove(&rel_path);
            self.script_dirs.insert(new_rel.clone());
            let old_dirs: Vec<_> = self.script_dirs.iter().cloned().collect();
            for d in old_dirs {
                let rewritten = Self::rewrite_prefix(&d, &rel_path, &new_rel);
                if rewritten != d {
                    self.script_dirs.remove(&d);
                    self.script_dirs.insert(rewritten);
                }
            }
            for rel in self.script_rels.values_mut() {
                *rel = Self::rewrite_prefix(rel, &rel_path, &new_rel);
            }
            return Ok(());
        }
        let script_id = self.find_script_id_by_rel(&rel_path)?;
        self.script_rels.insert(script_id, new_rel);
        Ok(())
    }

    fn apply_move_script_sibling(
        &mut self,
        _rel_path: String,
        _direction: i8,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn apply_copy_script(
        &mut self,
        src_rel: String,
        dest_parent_rel: String,
        new_name: String,
        new_script_id: Uuid,
    ) -> anyhow::Result<()> {
        validate_storage_name(&new_name)?;
        self.ensure_parent_script_dir(&dest_parent_rel)?;
        let src_id = self.find_script_id_by_rel(&src_rel)?;
        let src = self
            .scripts
            .get(&src_id)
            .with_context(|| format!("script {src_id} not found"))?
            .clone();
        if self.scripts.contains_key(&new_script_id) {
            bail!("script already exists: {new_script_id}");
        }
        let file_rel = Self::memory_rel_join(&dest_parent_rel, &format!("{new_name}.json"));
        if self.script_rels.values().any(|r| r == &file_rel) {
            bail!("script file already exists: {file_rel}");
        }
        let mut copied = src;
        copied.id = new_script_id;
        self.scripts.insert(new_script_id, copied);
        self.script_rels.insert(new_script_id, file_rel);
        Ok(())
    }

    fn apply_update_script_title(
        &mut self,
        script_id: Uuid,
        title: String,
    ) -> anyhow::Result<()> {
        let now = self.now;
        let script = self.script_mut(script_id)?;
        script.title = title;
        script.meta.updated_at = now;
        Ok(())
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

    fn apply_delete_script_block(
        &mut self,
        script_id: Uuid,
        block_id: Uuid,
    ) -> anyhow::Result<()> {
        let now = self.now;
        let script = self.script_mut(script_id)?;
        let idx = script
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("script block {block_id} not found"))?;
        script.remove_block(idx)?;
        script.meta.updated_at = now;
        Ok(())
    }

    fn apply_move_script_block(
        &mut self,
        script_id: Uuid,
        block_id: Uuid,
        to_index: usize,
    ) -> anyhow::Result<()> {
        let now = self.now;
        let script = self.script_mut(script_id)?;
        let from = script
            .blocks
            .iter()
            .position(|b| b.id == block_id)
            .with_context(|| format!("script block {block_id} not found"))?;
        script.move_block(from, to_index)?;
        script.meta.updated_at = now;
        Ok(())
    }

    fn apply_update_project_meta(
        &mut self,
        title: Option<String>,
        style_guide: Option<String>,
        synopsis: Option<String>,
    ) -> anyhow::Result<()> {
        if let Some(t) = title {
            self.title = t;
        }
        if let Some(s) = style_guide {
            self.style_guide = s;
        }
        if let Some(s) = synopsis {
            self.synopsis = s;
        }
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
            } => {
                self.apply_create_script(
                    title.clone(),
                    parent_rel.clone(),
                    *script_id,
                    name.clone(),
                )?;
                Ok(())
            }
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
                parent_rel: String::new(),
                script_id: None,
                name: None,
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

    #[test]
    fn apply_create_chapter_directory_and_file() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        mutator
            .apply(&AiIntent::CreateChapterDirectory {
                intent_id: Uuid::nil(),
                parent_rel: String::new(),
                name: "vol-001第一卷".into(),
            })
            .unwrap();
        assert_eq!(mutator.list_dirs(), vec!["vol-001第一卷".to_string()]);

        let chapter_id = Uuid::from_u128(42);
        mutator
            .apply(&AiIntent::CreateChapterFile {
                intent_id: Uuid::nil(),
                chapter_id,
                parent_rel: "vol-001第一卷".into(),
                name: "ch-001开篇".into(),
                title: "开篇".into(),
            })
            .unwrap();

        assert_eq!(
            mutator.chapter_rel(chapter_id),
            Some("vol-001第一卷/ch-001开篇.json")
        );
        assert_eq!(mutator.get_chapter(chapter_id).unwrap().title, "开篇");
        assert!(mutator.chapter_blocks(chapter_id).unwrap().is_empty());
    }

    #[test]
    fn create_chapter_file_rejects_missing_parent() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        let err = mutator
            .apply(&AiIntent::CreateChapterFile {
                intent_id: Uuid::nil(),
                chapter_id: Uuid::from_u128(7),
                parent_rel: "missing".into(),
                name: "ch-001".into(),
                title: "一".into(),
            })
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn apply_update_project_meta() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        mutator
            .apply(&AiIntent::UpdateProjectMeta {
                intent_id: Uuid::nil(),
                title: Some("新书名".into()),
                style_guide: Some("冷峻".into()),
                synopsis: Some("简介".into()),
            })
            .unwrap();
        assert_eq!(mutator.title, "新书名");
        assert_eq!(mutator.style_guide, "冷峻");
        assert_eq!(mutator.synopsis, "简介");
    }

    #[test]
    fn apply_delete_block() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        let ch_id = chapter_id();
        mutator.ensure_chapter(ch_id, "第一章");
        mutator
            .apply(&AiIntent::CreateBlock {
                intent_id: Uuid::nil(),
                chapter_id: ch_id,
                block_type: BlockType::Narration,
                content: "要删".into(),
                speaker: None,
                after_block_id: None,
            })
            .unwrap();
        let block_id = mutator.chapter_blocks(ch_id).unwrap()[0].id;
        mutator
            .apply(&AiIntent::DeleteBlock {
                intent_id: Uuid::nil(),
                chapter_id: ch_id,
                block_id,
            })
            .unwrap();
        assert!(mutator.chapter_blocks(ch_id).unwrap().is_empty());
    }

    #[test]
    fn apply_delete_outline_entry() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        mutator
            .apply(&AiIntent::CreateOutlineEntry {
                intent_id: Uuid::nil(),
                category: OutlineCategory::Character,
                key: "配角".into(),
                fields: None,
            })
            .unwrap();
        let id = mutator.get_outline()[0].id;
        mutator
            .apply(&AiIntent::DeleteOutlineEntry {
                intent_id: Uuid::nil(),
                id,
            })
            .unwrap();
        assert!(mutator.get_outline().is_empty());
    }

    #[test]
    fn apply_create_script_directory() {
        let mut mutator = InMemoryMutator::with_now(fixed_now());
        mutator
            .apply(&AiIntent::CreateScriptDirectory {
                intent_id: Uuid::nil(),
                parent_rel: String::new(),
                name: "ep-001".into(),
            })
            .unwrap();
        assert_eq!(mutator.list_script_dirs(), vec!["ep-001".to_string()]);
    }
}
