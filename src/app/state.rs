use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};

use crate::models::{
    Block, BlockFocus, BlockMultiSelect, BlockType, Chapter, OutlineCategory, OutlineEntry, Project,
};
use crate::services::autosave::SaveAction;
use crate::services::{
    ChapterStats, DebounceTimer, FullTextHit, SaveTrigger, SearchMode, action_for, count_chapter,
    filter_chapter_tree_by_name, filter_outline_by_name, search_full_text, update_book_total,
};
use crate::storage::{
    ChapterTreeNode, MoveDirection, RelPath, add_recent_project, copy_chapter, create_chapter_file,
    create_directory, create_outline_entry, create_project, delete_node, delete_outline_entry,
    find_node_by_chapter_id, is_nonempty_directory, list_outline_entries, load_chapter,
    load_config_from, load_project, move_node, move_sibling, outline_entry_path, rename_node,
    rename_outline_entry, resolve_rel, save_chapter, save_config_to, save_outline_entry,
    save_project, scan_chapter_tree,
};
use uuid::Uuid;

/// 左侧栏当前 Tab。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarTab {
    #[default]
    Chapters,
    Outline,
}

impl SidebarTab {
    pub fn as_str(self) -> &'static str {
        match self {
            SidebarTab::Chapters => "chapters",
            SidebarTab::Outline => "outline",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "outline" => SidebarTab::Outline,
            _ => SidebarTab::Chapters,
        }
    }
}

/// 工作区 UI 会话状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceUi {
    pub sidebar_tab: SidebarTab,
    pub ai_panel_open: bool,
    pub sidebar_visible: bool,
    /// 相对 `chapters/` 的已展开目录节点。
    pub expanded_nodes: HashSet<String>,
    /// 当前选中的树节点相对路径（目录或章节）。
    pub selected_node: Option<String>,
}

impl Default for WorkspaceUi {
    fn default() -> Self {
        Self {
            sidebar_tab: SidebarTab::Chapters,
            ai_panel_open: false,
            sidebar_visible: true,
            expanded_nodes: HashSet::new(),
            selected_node: None,
        }
    }
}

/// 应用协调状态：当前项目、章节树、配置、dirty 与 UI。
#[derive(Debug, Clone)]
pub struct AppState {
    pub project: Option<Project>,
    pub project_dir: Option<PathBuf>,
    pub config: crate::storage::AppConfig,
    /// 配置文件路径（测试可注入临时路径）。
    pub config_path: PathBuf,
    pub ui: WorkspaceUi,
    pub dirty: bool,
    /// 最近一次保存失败的提示（保留 dirty）。
    pub save_error: Option<String>,
    /// 扫描得到的章节树。
    pub chapter_tree: Vec<ChapterTreeNode>,
    /// 当前打开的章节内容。
    pub current_chapter: Option<Chapter>,
    /// 当前章节文件相对路径。
    pub current_chapter_path: Option<RelPath>,
    /// 当前章在上次成功保存时的总字数（全书增量基线）。
    pub chapter_word_count_baseline: u64,
    /// 段落块焦点状态机。
    pub block_focus: BlockFocus,
    /// 多选相邻块（合并用）；与单选焦点并存。
    pub block_multi_select: Option<BlockMultiSelect>,
    /// 文本防抖计时（UI 用单调毫秒驱动）。
    pub debounce: DebounceTimer,
    /// 当前项目大纲条目缓存。
    pub outline_entries: Vec<OutlineEntry>,
    /// 中心区正在编辑的大纲条目。
    pub current_outline: Option<OutlineEntry>,
    /// 搜索框查询。
    pub search_query: String,
    /// 搜索模式。
    pub search_mode: SearchMode,
    /// 全文搜索结果。
    pub full_text_hits: Vec<FullTextHit>,
}

impl AppState {
    /// 从默认平台配置路径加载（GUI 入口用）。
    pub fn load() -> Result<Self> {
        let config_path = crate::storage::config_path()?;
        Self::load_from(config_path)
    }

    /// 从指定配置路径加载。
    pub fn load_from(config_path: impl Into<PathBuf>) -> Result<Self> {
        let config_path = config_path.into();
        let config = load_config_from(&config_path)?;
        Ok(Self {
            project: None,
            project_dir: None,
            config,
            config_path,
            ui: WorkspaceUi::default(),
            dirty: false,
            save_error: None,
            chapter_tree: Vec::new(),
            current_chapter: None,
            current_chapter_path: None,
            chapter_word_count_baseline: 0,
            block_focus: BlockFocus::Idle,
            block_multi_select: None,
            debounce: DebounceTimer::with_default_delay(),
            outline_entries: Vec::new(),
            current_outline: None,
            search_query: String::new(),
            search_mode: SearchMode::default(),
            full_text_hits: Vec::new(),
        })
    }

    /// 展开 `projects_root` 中的 `~`。
    pub fn expanded_projects_root(&self) -> Result<PathBuf> {
        expand_user_path(&self.config.projects_root)
    }

    pub fn max_depth(&self) -> u32 {
        self.project
            .as_ref()
            .map(|p| p.settings.max_depth)
            .unwrap_or(3)
    }

    /// 在 projects_root 下新建项目并记入最近列表。
    pub fn new_project(&mut self, title: impl Into<String>, now: DateTime<Utc>) -> Result<()> {
        self.flush_save(SaveTrigger::BeforeLeave, now)?;
        let title = title.into();
        let root = self.expanded_projects_root()?;
        let (project_dir, project) = create_project(&root, &title, now)?;
        self.set_current_project(project_dir, project, now)?;
        Ok(())
    }

    /// 打开已有项目目录并记入最近列表。
    pub fn open_project(&mut self, path: impl AsRef<Path>, now: DateTime<Utc>) -> Result<()> {
        self.flush_save(SaveTrigger::BeforeLeave, now)?;
        let project_dir = path.as_ref().to_path_buf();
        let project = load_project(&project_dir)
            .with_context(|| format!("failed to open project {}", project_dir.display()))?;
        self.set_current_project(project_dir, project, now)?;
        Ok(())
    }

    /// 打开最近列表第一条（若有）。
    pub fn open_most_recent(&mut self, now: DateTime<Utc>) -> Result<()> {
        let Some(recent) = self.config.recent_projects.first().cloned() else {
            bail!("no recent projects");
        };
        let path = expand_user_path(&recent.path)?;
        self.open_project(path, now)
    }

    pub fn toggle_ai_panel(&mut self) {
        self.ui.ai_panel_open = !self.ui.ai_panel_open;
        let _ = self.persist_ui_state();
    }

    pub fn toggle_sidebar(&mut self) {
        self.ui.sidebar_visible = !self.ui.sidebar_visible;
    }

    pub fn set_sidebar_tab(&mut self, tab: SidebarTab) {
        self.ui.sidebar_tab = tab;
        let _ = self.persist_ui_state();
    }

    /// 更新应用配置中的项目根目录并落盘。
    pub fn set_projects_root(&mut self, root: impl Into<String>) -> Result<()> {
        self.config.projects_root = root.into();
        save_config_to(&self.config_path, &self.config)?;
        Ok(())
    }

    /// 更新当前项目 max_depth 并落盘。
    pub fn set_max_depth(&mut self, max_depth: u32, now: DateTime<Utc>) -> Result<()> {
        let project = self.project.as_mut().context("no project open")?;
        project.settings.max_depth = max_depth.max(1);
        project.updated_at = now;
        self.persist_project()
    }

    pub fn current_title(&self) -> &str {
        self.project
            .as_ref()
            .map(|p| p.title.as_str())
            .unwrap_or("未打开项目")
    }

    pub fn refresh_chapter_tree(&mut self) -> Result<()> {
        let Some(project_dir) = self.project_dir.as_ref() else {
            self.chapter_tree.clear();
            return Ok(());
        };
        self.chapter_tree = scan_chapter_tree(project_dir)?;
        Ok(())
    }

    pub fn refresh_outline_entries(&mut self) -> Result<()> {
        let Some(project_dir) = self.project_dir.as_ref() else {
            self.outline_entries.clear();
            return Ok(());
        };
        self.outline_entries = list_outline_entries(project_dir)?;
        Ok(())
    }

    /// 名称过滤后的章节树（全文模式或空查询时返回原树）。
    pub fn displayed_chapter_tree(&self) -> Vec<ChapterTreeNode> {
        if self.search_mode != SearchMode::NameFilter || self.search_query.trim().is_empty() {
            return self.chapter_tree.clone();
        }
        filter_chapter_tree_by_name(&self.chapter_tree, &self.search_query)
    }

    /// 名称过滤后的大纲列表。
    pub fn displayed_outline_entries(&self) -> Vec<OutlineEntry> {
        if self.search_mode != SearchMode::NameFilter || self.search_query.trim().is_empty() {
            return self.outline_entries.clone();
        }
        filter_outline_by_name(&self.outline_entries, &self.search_query)
    }

    pub fn set_search_query(&mut self, query: impl Into<String>) -> Result<()> {
        self.search_query = query.into();
        self.refresh_search_results()
    }

    pub fn set_search_mode(&mut self, mode: SearchMode) -> Result<()> {
        self.search_mode = mode;
        self.refresh_search_results()
    }

    pub fn refresh_search_results(&mut self) -> Result<()> {
        self.full_text_hits.clear();
        if self.search_mode != SearchMode::FullText {
            return Ok(());
        }
        let Some(project_dir) = self.project_dir.as_ref() else {
            return Ok(());
        };
        self.full_text_hits = search_full_text(project_dir, &self.search_query)?;
        Ok(())
    }

    /// 打开全文命中：加载章节并选中对应块。
    pub fn open_full_text_hit(&mut self, index: usize, now: DateTime<Utc>) -> Result<()> {
        let hit = self
            .full_text_hits
            .get(index)
            .cloned()
            .context("full-text hit index out of range")?;
        self.select_chapter(&hit.chapter_rel, now)?;
        self.current_outline = None;
        self.ui.sidebar_tab = SidebarTab::Chapters;
        // 按 block_id 定位；找不到则回退到记录的 index
        let block_index = self
            .current_chapter
            .as_ref()
            .and_then(|ch| ch.blocks.iter().position(|b| b.id == hit.block_id))
            .unwrap_or(hit.block_index);
        self.block_focus = BlockFocus::Selected { index: block_index };
        self.block_multi_select = None;
        Ok(())
    }

    pub fn select_outline(&mut self, id: Uuid, now: DateTime<Utc>) -> Result<()> {
        self.flush_save(SaveTrigger::BeforeLeave, now)?;
        let entry = self
            .outline_entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .context("outline entry not found")?;
        self.current_outline = Some(entry);
        self.current_chapter = None;
        self.current_chapter_path = None;
        self.chapter_word_count_baseline = 0;
        self.block_focus = BlockFocus::Idle;
        self.block_multi_select = None;
        self.ui.selected_node = None;
        self.dirty = false;
        self.save_error = None;
        self.debounce.cancel();
        Ok(())
    }

    pub fn create_outline(
        &mut self,
        key: &str,
        category: OutlineCategory,
        now: DateTime<Utc>,
    ) -> Result<Uuid> {
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        self.flush_save(SaveTrigger::BeforeLeave, now)?;
        let entry = create_outline_entry(&project_dir, key, category, now)?;
        let id = entry.id;
        self.refresh_outline_entries()?;
        self.current_outline = Some(entry);
        self.dirty = false;
        self.touch_project(now)?;
        Ok(id)
    }

    pub fn delete_outline(&mut self, id: Uuid, now: DateTime<Utc>) -> Result<()> {
        let entry = self
            .outline_entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .context("outline entry not found")?;
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        delete_outline_entry(&project_dir, entry.category, &entry.key)?;
        if self.current_outline.as_ref().map(|e| e.id) == Some(id) {
            self.current_outline = None;
            self.dirty = false;
            self.debounce.cancel();
        }
        self.refresh_outline_entries()?;
        self.touch_project(now)?;
        Ok(())
    }

    pub fn rename_outline(&mut self, id: Uuid, new_key: &str, now: DateTime<Utc>) -> Result<()> {
        let entry = self
            .outline_entries
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .context("outline entry not found")?;
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        self.flush_save(SaveTrigger::BeforeLeave, now)?;
        let renamed = rename_outline_entry(&project_dir, entry.category, &entry.key, new_key, now)?;
        self.refresh_outline_entries()?;
        if self.current_outline.as_ref().map(|e| e.id) == Some(id) {
            self.current_outline = Some(renamed);
            self.dirty = false;
        }
        self.touch_project(now)?;
        Ok(())
    }

    pub fn set_outline_field(
        &mut self,
        field_key: &str,
        value: String,
        now: DateTime<Utc>,
        now_ms: u64,
    ) -> Result<()> {
        let entry = self
            .current_outline
            .as_mut()
            .context("no outline selected")?;
        entry.fields.insert(field_key.to_string(), value);
        entry.meta.updated_at = now;
        self.on_text_edit(now_ms);
        Ok(())
    }

    pub fn add_outline_field(
        &mut self,
        field_key: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let key = field_key.into();
        let entry = self
            .current_outline
            .as_mut()
            .context("no outline selected")?;
        if entry.fields.contains_key(&key) {
            bail!("field already exists: {key}");
        }
        entry.fields.insert(key, String::new());
        entry.meta.updated_at = now;
        self.dirty = true;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn remove_outline_field(&mut self, field_key: &str, now: DateTime<Utc>) -> Result<()> {
        let entry = self
            .current_outline
            .as_mut()
            .context("no outline selected")?;
        entry.fields.remove(field_key);
        entry.meta.updated_at = now;
        self.dirty = true;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn rename_outline_field(
        &mut self,
        old_key: &str,
        new_key: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        if old_key == new_key {
            return Ok(());
        }
        let entry = self
            .current_outline
            .as_mut()
            .context("no outline selected")?;
        if entry.fields.contains_key(new_key) {
            bail!("field already exists: {new_key}");
        }
        let value = entry
            .fields
            .remove(old_key)
            .with_context(|| format!("field not found: {old_key}"))?;
        entry.fields.insert(new_key.to_string(), value);
        entry.meta.updated_at = now;
        self.dirty = true;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn toggle_expanded(&mut self, rel_path: &str) -> Result<()> {
        if self.ui.expanded_nodes.contains(rel_path) {
            self.ui.expanded_nodes.remove(rel_path);
        } else {
            self.ui.expanded_nodes.insert(rel_path.to_string());
        }
        self.persist_ui_state()?;
        Ok(())
    }

    pub fn is_expanded(&self, rel_path: &str) -> bool {
        self.ui.expanded_nodes.contains(rel_path)
    }

    /// 选中目录节点（仅高亮，不加载章节）。
    pub fn select_directory(&mut self, rel_path: &str) {
        self.ui.selected_node = Some(rel_path.to_string());
    }

    /// 选中并加载章节到中心编辑区。
    pub fn select_chapter(&mut self, rel_path: &str, now: DateTime<Utc>) -> Result<()> {
        // 切换前同步保存当前章
        if self.current_chapter_path.as_deref() != Some(rel_path) {
            self.flush_save(SaveTrigger::BeforeLeave, now)?;
        }

        let project_dir = self.project_dir.as_ref().context("no project open")?;
        let path = resolve_rel(project_dir, rel_path)?;
        let chapter = load_chapter(&path)?;
        let chapter_id = chapter.id;
        let baseline = self.chapter_stats_for(&chapter).total_words();

        self.current_chapter = Some(chapter);
        self.current_chapter_path = Some(rel_path.to_string());
        self.ui.selected_node = Some(rel_path.to_string());
        self.current_outline = None;
        self.dirty = false;
        self.save_error = None;
        self.chapter_word_count_baseline = baseline;
        self.block_focus = BlockFocus::Idle;
        self.block_multi_select = None;
        self.debounce.cancel();

        if let Some(project) = self.project.as_mut() {
            project.last_opened_chapter = Some(chapter_id);
            project.updated_at = now;
        }
        self.persist_project()?;
        Ok(())
    }

    pub fn create_dir_under(
        &mut self,
        parent_rel: &str,
        name: &str,
        now: DateTime<Utc>,
    ) -> Result<RelPath> {
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        let max = self.max_depth();
        let rel = create_directory(&project_dir, parent_rel, name, max)?;
        if !parent_rel.is_empty() {
            self.ui.expanded_nodes.insert(parent_rel.to_string());
        }
        self.refresh_chapter_tree()?;
        self.touch_project(now)?;
        self.persist_ui_state()?;
        Ok(rel)
    }

    pub fn create_chapter_under(
        &mut self,
        parent_rel: &str,
        name: &str,
        title: &str,
        now: DateTime<Utc>,
    ) -> Result<RelPath> {
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        let max = self.max_depth();
        let chapter = Chapter::new(title, now);
        let rel = create_chapter_file(&project_dir, parent_rel, name, &chapter, max)?;
        if !parent_rel.is_empty() {
            self.ui.expanded_nodes.insert(parent_rel.to_string());
        }
        self.refresh_chapter_tree()?;
        self.touch_project(now)?;
        self.persist_ui_state()?;
        self.select_chapter(&rel, now)?;
        Ok(rel)
    }

    pub fn rename_selected(&mut self, new_name: &str, now: DateTime<Utc>) -> Result<RelPath> {
        let rel = self.ui.selected_node.clone().context("no node selected")?;
        self.rename_at(&rel, new_name, now)
    }

    pub fn rename_at(
        &mut self,
        rel_path: &str,
        new_name: &str,
        now: DateTime<Utc>,
    ) -> Result<RelPath> {
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        let new_rel = rename_node(&project_dir, rel_path, new_name)?;
        self.remap_paths_after_rename(rel_path, &new_rel);
        self.refresh_chapter_tree()?;
        self.touch_project(now)?;
        self.persist_ui_state()?;
        Ok(new_rel)
    }

    pub fn delete_at(&mut self, rel_path: &str, now: DateTime<Utc>) -> Result<()> {
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        delete_node(&project_dir, rel_path)?;
        self.clear_paths_under(rel_path);
        self.refresh_chapter_tree()?;
        self.touch_project(now)?;
        self.persist_ui_state()?;
        Ok(())
    }

    pub fn directory_is_nonempty(&self, rel_path: &str) -> Result<bool> {
        let project_dir = self.project_dir.as_ref().context("no project open")?;
        is_nonempty_directory(project_dir, rel_path)
    }

    pub fn copy_chapter_at(
        &mut self,
        src_rel: &str,
        dest_parent_rel: &str,
        new_name: &str,
        now: DateTime<Utc>,
    ) -> Result<RelPath> {
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        let max = self.max_depth();
        let (rel, _) = copy_chapter(&project_dir, src_rel, dest_parent_rel, new_name, max, now)?;
        if !dest_parent_rel.is_empty() {
            self.ui.expanded_nodes.insert(dest_parent_rel.to_string());
        }
        self.refresh_chapter_tree()?;
        self.touch_project(now)?;
        self.persist_ui_state()?;
        Ok(rel)
    }

    pub fn move_node_to(
        &mut self,
        src_rel: &str,
        dest_parent_rel: &str,
        now: DateTime<Utc>,
    ) -> Result<RelPath> {
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        let max = self.max_depth();
        let new_rel = move_node(&project_dir, src_rel, dest_parent_rel, None, max)?;
        self.remap_paths_after_rename(src_rel, &new_rel);
        if !dest_parent_rel.is_empty() {
            self.ui.expanded_nodes.insert(dest_parent_rel.to_string());
        }
        self.refresh_chapter_tree()?;
        self.touch_project(now)?;
        self.persist_ui_state()?;
        Ok(new_rel)
    }

    pub fn move_selected_sibling(
        &mut self,
        direction: MoveDirection,
        now: DateTime<Utc>,
    ) -> Result<RelPath> {
        let rel = self.ui.selected_node.clone().context("no node selected")?;
        let project_dir = self
            .project_dir
            .as_ref()
            .context("no project open")?
            .clone();
        let new_rel = move_sibling(&project_dir, &rel, direction)?;
        self.remap_paths_after_rename(&rel, &new_rel);
        self.refresh_chapter_tree()?;
        self.touch_project(now)?;
        Ok(new_rel)
    }

    /// 更新当前章节标题并写盘。
    pub fn set_current_chapter_title(&mut self, title: impl Into<String>) -> Result<()> {
        let title = title.into();
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.title = title;
        self.dirty = true;
        self.save_now()?;
        self.refresh_chapter_tree()?;
        Ok(())
    }

    /// 当前章实时统计（尊重项目排除类型）。
    pub fn current_chapter_stats(&self) -> ChapterStats {
        match self.current_chapter.as_ref() {
            Some(ch) => self.chapter_stats_for(ch),
            None => ChapterStats::default(),
        }
    }

    fn chapter_stats_for(&self, chapter: &Chapter) -> ChapterStats {
        let exclude = self
            .project
            .as_ref()
            .map(|p| p.settings.word_count_exclude_types.as_slice())
            .unwrap_or(&[BlockType::Note, BlockType::SceneBreak]);
        count_chapter(chapter, exclude)
    }

    /// 全书总字数（含未保存的本章增量预览）。
    pub fn displayed_book_total(&self) -> u64 {
        let book = self
            .project
            .as_ref()
            .map(|p| p.total_word_count)
            .unwrap_or(0);
        let current = self.current_chapter_stats().total_words();
        update_book_total(book, self.chapter_word_count_baseline, current)
    }

    /// 文本编辑：标记 dirty 并按防抖策略处理。
    pub fn on_text_edit(&mut self, now_ms: u64) {
        self.dirty = true;
        if matches!(
            action_for(SaveTrigger::TextEdit),
            SaveAction::ScheduleDebounce
        ) {
            self.debounce.schedule_from(now_ms);
        }
    }

    /// UI 时钟滴答：若防抖到期则保存。
    pub fn tick_autosave(&mut self, now_ms: u64, _now: DateTime<Utc>) -> Result<bool> {
        if self.debounce.take_if_due(now_ms) {
            if self.dirty {
                self.save_now()?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// 菜单 / Cmd+S：立即保存。
    pub fn save_manual(&mut self, now: DateTime<Utc>) -> Result<()> {
        self.flush_save(SaveTrigger::Manual, now)
    }

    /// 退出前保存。
    pub fn save_before_quit(&mut self, now: DateTime<Utc>) -> Result<()> {
        self.flush_save(SaveTrigger::BeforeLeave, now)
    }

    fn flush_save(&mut self, trigger: SaveTrigger, _now: DateTime<Utc>) -> Result<()> {
        match action_for(trigger) {
            SaveAction::ScheduleDebounce => Ok(()),
            SaveAction::SaveNow => {
                self.debounce.take_pending();
                if self.current_chapter.is_none() && self.current_outline.is_none() {
                    return Ok(());
                }
                // 手动保存始终尝试；其余仅在 dirty 时写盘
                if !self.dirty && trigger != SaveTrigger::Manual {
                    return Ok(());
                }
                self.save_now()
            }
        }
    }

    /// 立即将当前章节/大纲与全书字数写盘。失败时保留 dirty 并记录错误。
    pub fn save_now(&mut self) -> Result<()> {
        let result = (|| -> Result<()> {
            if let Some(entry) = self.current_outline.clone() {
                let project_dir = self
                    .project_dir
                    .as_ref()
                    .context("no project open")?
                    .clone();
                let path = outline_entry_path(&project_dir, entry.category, &entry.key)?;
                save_outline_entry(&path, &entry)?;
                // 同步缓存
                if let Some(slot) = self.outline_entries.iter_mut().find(|e| e.id == entry.id) {
                    *slot = entry.clone();
                }
                self.current_outline = Some(entry);
            }

            if self.current_chapter.is_some() {
                let project_dir = self
                    .project_dir
                    .as_ref()
                    .context("no project open")?
                    .clone();
                let rel = self
                    .current_chapter_path
                    .as_ref()
                    .context("no chapter open")?
                    .clone();
                let chapter = self.current_chapter.as_ref().context("no chapter open")?;
                let new_count = self.chapter_stats_for(chapter).total_words();
                let path = resolve_rel(&project_dir, &rel)?;
                save_chapter(&path, chapter)?;

                if let Some(project) = self.project.as_mut() {
                    project.total_word_count = update_book_total(
                        project.total_word_count,
                        self.chapter_word_count_baseline,
                        new_count,
                    );
                    project.updated_at = Utc::now();
                }
                self.chapter_word_count_baseline = new_count;
                self.persist_project()?;
            }

            self.dirty = false;
            self.save_error = None;
            Ok(())
        })();

        if let Err(ref err) = result {
            self.save_error = Some(format!("{err:#}"));
            // 保留 dirty
        }
        result
    }

    // —— 块操作（结构性变更 → 立即保存）——

    pub fn insert_block_at(
        &mut self,
        index: usize,
        block_type: BlockType,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.insert_block(index, Block::new(block_type, String::new(), now))?;
        self.dirty = true;
        self.block_focus = BlockFocus::Selected { index };
        self.block_multi_select = None;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn delete_block_at(&mut self, index: usize, now: DateTime<Utc>) -> Result<()> {
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.remove_block(index)?;
        self.dirty = true;
        self.block_focus = self.block_focus.clone().clamp_to_len(chapter.blocks.len());
        self.block_multi_select = None;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn set_block_type_at(
        &mut self,
        index: usize,
        block_type: BlockType,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.set_block_type(index, block_type, now)?;
        self.dirty = true;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn set_block_speaker_at(
        &mut self,
        index: usize,
        speaker: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.set_speaker(index, speaker, now)?;
        self.dirty = true;
        self.on_text_edit(0); // 防抖：speaker 视为文本
        Ok(())
    }

    pub fn set_block_content_at(
        &mut self,
        index: usize,
        content: String,
        now: DateTime<Utc>,
        now_ms: u64,
    ) -> Result<()> {
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.set_block_content(index, content, now)?;
        self.on_text_edit(now_ms);
        Ok(())
    }

    pub fn split_block_at_cursor(
        &mut self,
        index: usize,
        byte_offset: usize,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.split_block_at(index, byte_offset, now)?;
        self.dirty = true;
        self.block_focus = BlockFocus::Editing { index: index + 1 };
        self.block_multi_select = None;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn merge_selected_blocks(&mut self, now: DateTime<Utc>) -> Result<()> {
        let range = self
            .block_multi_select
            .clone()
            .context("no multi-selection for merge")?;
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.merge_blocks(range.start, range.end, now)?;
        self.dirty = true;
        self.block_focus = BlockFocus::Selected { index: range.start };
        self.block_multi_select = None;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn swap_block_at(&mut self, index: usize, up: bool, now: DateTime<Utc>) -> Result<()> {
        let chapter = self.current_chapter.as_mut().context("no chapter open")?;
        chapter.swap_block(index, up)?;
        let new_index = if up { index - 1 } else { index + 1 };
        self.dirty = true;
        self.block_focus = BlockFocus::Selected { index: new_index };
        self.block_multi_select = None;
        self.flush_save(SaveTrigger::StructuralChange, now)
    }

    pub fn click_block(&mut self, index: usize) {
        self.block_focus = self.block_focus.clone().click_block(index);
        self.block_multi_select = None;
    }

    pub fn click_editor_outside(&mut self) {
        self.block_focus = self.block_focus.clone().click_outside();
        self.block_multi_select = None;
    }

    pub fn escape_block_focus(&mut self) {
        self.block_focus = self.block_focus.clone().escape();
    }

    pub fn move_block_focus(&mut self, delta: isize) {
        let count = self
            .current_chapter
            .as_ref()
            .map(|c| c.blocks.len())
            .unwrap_or(0);
        self.block_focus = self.block_focus.clone().move_selection(delta, count);
        self.block_multi_select = None;
    }

    pub fn set_multi_select(&mut self, a: usize, b: usize) {
        self.block_multi_select = Some(BlockMultiSelect::new(a, b));
        self.block_focus = BlockFocus::Selected { index: a.min(b) };
    }

    fn set_current_project(
        &mut self,
        project_dir: PathBuf,
        project: Project,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.ui.sidebar_tab = SidebarTab::parse(&project.ui_state.sidebar_tab);
        self.ui.ai_panel_open = project.ui_state.ai_panel_open;
        self.ui.expanded_nodes = project.ui_state.expanded_nodes.iter().cloned().collect();
        self.ui.selected_node = None;
        self.current_chapter = None;
        self.current_chapter_path = None;
        self.chapter_word_count_baseline = 0;
        self.block_focus = BlockFocus::Idle;
        self.block_multi_select = None;
        self.debounce.cancel();
        self.save_error = None;
        self.outline_entries.clear();
        self.current_outline = None;
        self.search_query.clear();
        self.full_text_hits.clear();

        let path_str = project_dir.to_string_lossy().into_owned();
        add_recent_project(&mut self.config, path_str, project.title.clone(), now);
        save_config_to(&self.config_path, &self.config)?;

        self.project_dir = Some(project_dir);
        self.project = Some(project);
        self.dirty = false;
        self.refresh_chapter_tree()?;
        self.refresh_outline_entries()?;
        let _ = self.refresh_search_results();

        // 恢复 last_opened_chapter
        let last_id = self.project.as_ref().and_then(|p| p.last_opened_chapter);
        if let Some(id) = last_id
            && let Some(node) = find_node_by_chapter_id(&self.chapter_tree, id).cloned()
        {
            let _ = self.select_chapter(&node.rel_path, now);
        }
        Ok(())
    }

    fn touch_project(&mut self, now: DateTime<Utc>) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            project.updated_at = now;
        }
        self.persist_project()
    }

    fn persist_ui_state(&mut self) -> Result<()> {
        if let Some(project) = self.project.as_mut() {
            let mut nodes: Vec<_> = self.ui.expanded_nodes.iter().cloned().collect();
            nodes.sort();
            project.ui_state.expanded_nodes = nodes;
            project.ui_state.sidebar_tab = self.ui.sidebar_tab.as_str().to_string();
            project.ui_state.ai_panel_open = self.ui.ai_panel_open;
        }
        self.persist_project()
    }

    fn persist_project(&self) -> Result<()> {
        let (Some(dir), Some(project)) = (self.project_dir.as_ref(), self.project.as_ref()) else {
            return Ok(());
        };
        save_project(dir, project)
    }

    fn remap_paths_after_rename(&mut self, old: &str, new: &str) {
        if self.ui.selected_node.as_deref() == Some(old)
            || self
                .ui
                .selected_node
                .as_ref()
                .is_some_and(|p| p.starts_with(&format!("{old}/")))
        {
            self.ui.selected_node = Some(remap_prefix(
                self.ui.selected_node.as_deref().unwrap_or(""),
                old,
                new,
            ));
        }
        if self.current_chapter_path.as_deref() == Some(old)
            || self
                .current_chapter_path
                .as_ref()
                .is_some_and(|p| p.starts_with(&format!("{old}/")))
        {
            self.current_chapter_path = Some(remap_prefix(
                self.current_chapter_path.as_deref().unwrap_or(""),
                old,
                new,
            ));
        }
        let expanded: Vec<_> = self.ui.expanded_nodes.drain().collect();
        for path in expanded {
            if path == old || path.starts_with(&format!("{old}/")) {
                self.ui.expanded_nodes.insert(remap_prefix(&path, old, new));
            } else {
                self.ui.expanded_nodes.insert(path);
            }
        }
    }

    fn clear_paths_under(&mut self, rel: &str) {
        let prefix = format!("{rel}/");
        self.ui
            .expanded_nodes
            .retain(|p| p != rel && !p.starts_with(&prefix));
        if self.ui.selected_node.as_deref() == Some(rel)
            || self
                .ui
                .selected_node
                .as_ref()
                .is_some_and(|p| p.starts_with(&prefix))
        {
            self.ui.selected_node = None;
        }
        if self.current_chapter_path.as_deref() == Some(rel)
            || self
                .current_chapter_path
                .as_ref()
                .is_some_and(|p| p.starts_with(&prefix))
        {
            self.current_chapter = None;
            self.current_chapter_path = None;
            self.chapter_word_count_baseline = 0;
            self.block_focus = BlockFocus::Idle;
            self.block_multi_select = None;
            if let Some(project) = self.project.as_mut() {
                project.last_opened_chapter = None;
            }
        }
    }
}

fn remap_prefix(path: &str, old: &str, new: &str) -> String {
    if path == old {
        new.to_string()
    } else if let Some(rest) = path.strip_prefix(&format!("{old}/")) {
        format!("{new}/{rest}")
    } else {
        path.to_string()
    }
}

/// 将以 `~/` 开头的路径展开为用户主目录。
pub fn expand_user_path(path: &str) -> Result<PathBuf> {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().context("cannot resolve home directory")?;
        Ok(home.join(rest))
    } else if path == "~" {
        dirs::home_dir().context("cannot resolve home directory")
    } else {
        Ok(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 30, 0, 0, 0).unwrap()
    }

    fn state_with_temp_root() -> (tempfile::TempDir, AppState) {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let projects_root = dir.path().join("novels");
        let mut state = AppState::load_from(&config_path).unwrap();
        state.config.projects_root = projects_root.to_string_lossy().into_owned();
        (dir, state)
    }

    #[test]
    fn new_project_creates_structure_and_reloads_via_open() {
        let (_dir, mut state) = state_with_temp_root();

        state.new_project("阶段1示例", now()).unwrap();

        let project_dir = state.project_dir.clone().expect("project_dir set");
        let title = state.project.as_ref().unwrap().title.clone();
        assert_eq!(title, "阶段1示例");
        assert!(project_dir.join("project.json").is_file());
        assert!(project_dir.join("chapters").is_dir());

        let config_path = state.config_path.clone();
        let mut reloaded = AppState::load_from(&config_path).unwrap();
        reloaded.config.projects_root = state.config.projects_root.clone();
        reloaded.open_project(&project_dir, now()).unwrap();

        assert_eq!(reloaded.project.as_ref().unwrap().title, "阶段1示例");
        assert_eq!(reloaded.project_dir.as_ref(), Some(&project_dir));
        assert!(!reloaded.dirty);
        assert_eq!(
            reloaded
                .config
                .recent_projects
                .first()
                .map(|r| r.title.as_str()),
            Some("阶段1示例")
        );
    }

    #[test]
    fn open_most_recent_uses_recent_list() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("最近甲", now()).unwrap();
        let path = state.project_dir.clone().unwrap();

        let mut fresh = AppState::load_from(&state.config_path).unwrap();
        fresh.config.projects_root = state.config.projects_root.clone();
        fresh.open_most_recent(now()).unwrap();

        assert_eq!(fresh.project_dir.as_ref(), Some(&path));
        assert_eq!(fresh.current_title(), "最近甲");
    }

    #[test]
    fn ai_panel_defaults_closed_and_toggles() {
        let (_dir, mut state) = state_with_temp_root();
        assert!(!state.ui.ai_panel_open);
        state.toggle_ai_panel();
        assert!(state.ui.ai_panel_open);
    }

    #[test]
    fn expand_user_path_keeps_absolute() {
        let p = expand_user_path("/tmp/novels").unwrap();
        assert_eq!(p, PathBuf::from("/tmp/novels"));
    }

    #[test]
    fn chapter_crud_and_restore_expanded_and_last_opened() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("章节树", now()).unwrap();

        let vol = state.create_dir_under("", "vol-001第一卷", now()).unwrap();
        let part = state.create_dir_under(&vol, "part-001上篇", now()).unwrap();
        let ch = state
            .create_chapter_under(&part, "ch-001开篇", "开篇", now())
            .unwrap();

        assert!(state.is_expanded(&vol));
        assert!(state.is_expanded(&part));
        assert_eq!(state.current_chapter.as_ref().unwrap().title, "开篇");
        assert_eq!(state.current_chapter_path.as_deref(), Some(ch.as_str()));
        assert_eq!(
            state.project.as_ref().unwrap().last_opened_chapter,
            state.current_chapter.as_ref().map(|c| c.id)
        );

        state.toggle_expanded(&vol).unwrap();
        assert!(!state.is_expanded(&vol));
        state.toggle_expanded(&vol).unwrap();

        let project_dir = state.project_dir.clone().unwrap();
        let chapter_id = state.current_chapter.as_ref().unwrap().id;
        let config_path = state.config_path.clone();
        let projects_root = state.config.projects_root.clone();

        let mut reloaded = AppState::load_from(&config_path).unwrap();
        reloaded.config.projects_root = projects_root;
        reloaded.open_project(&project_dir, now()).unwrap();

        assert!(reloaded.is_expanded(&vol));
        assert!(reloaded.is_expanded(&part));
        assert_eq!(
            reloaded.current_chapter.as_ref().map(|c| c.id),
            Some(chapter_id)
        );
        assert_eq!(reloaded.current_chapter.as_ref().unwrap().title, "开篇");
        assert_eq!(reloaded.chapter_tree.len(), 1);
    }

    #[test]
    fn delete_nonempty_dir_clears_selection() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("删目录", now()).unwrap();
        state.create_dir_under("", "vol-001", now()).unwrap();
        state
            .create_chapter_under("vol-001", "ch-001", "一", now())
            .unwrap();
        assert!(state.directory_is_nonempty("vol-001").unwrap());
        state.delete_at("vol-001", now()).unwrap();
        assert!(state.chapter_tree.is_empty());
        assert!(state.current_chapter.is_none());
        assert!(
            state
                .project
                .as_ref()
                .unwrap()
                .last_opened_chapter
                .is_none()
        );
    }

    #[test]
    fn move_sibling_and_copy_via_app_state() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("移动", now()).unwrap();
        state
            .create_chapter_under("", "ch-001", "一", now())
            .unwrap();
        state
            .create_chapter_under("", "ch-002", "二", now())
            .unwrap();
        state.ui.selected_node = Some("ch-002.json".into());
        let new_rel = state
            .move_selected_sibling(MoveDirection::Up, now())
            .unwrap();
        assert_eq!(new_rel, "ch-001.json");

        let copied = state
            .copy_chapter_at("ch-001.json", "", "ch-003副本", now())
            .unwrap();
        assert_eq!(copied, "ch-003副本.json");
        assert_eq!(state.chapter_tree.len(), 3);
    }

    #[test]
    fn set_chapter_title_persists() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("标题", now()).unwrap();
        state
            .create_chapter_under("", "ch-001", "旧标题", now())
            .unwrap();
        state.set_current_chapter_title("新标题").unwrap();
        assert_eq!(state.current_chapter.as_ref().unwrap().title, "新标题");
        let path = resolve_rel(state.project_dir.as_ref().unwrap(), "ch-001.json").unwrap();
        let loaded = load_chapter(&path).unwrap();
        assert_eq!(loaded.title, "新标题");
    }

    #[test]
    fn block_ops_save_immediately_and_update_word_count() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("块编辑", now()).unwrap();
        state
            .create_chapter_under("", "ch-001", "一", now())
            .unwrap();

        state
            .set_block_content_at(0, "汉字测试。".into(), now(), 0)
            .unwrap();
        assert!(state.dirty);
        assert!(state.debounce.is_pending());

        // 防抖到期
        state.tick_autosave(500, now()).unwrap();
        assert!(!state.dirty);
        assert_eq!(state.current_chapter_stats().chars.han, 4);
        assert_eq!(state.current_chapter_stats().chars.punct_space, 1);
        assert_eq!(state.project.as_ref().unwrap().total_word_count, 5);

        // 结构性：插入立即保存
        state
            .insert_block_at(1, BlockType::Dialogue, now())
            .unwrap();
        assert!(!state.dirty);
        assert_eq!(state.current_chapter.as_ref().unwrap().blocks.len(), 2);

        state
            .set_block_content_at(1, "对话内容".into(), now(), 1000)
            .unwrap();
        state.save_manual(now()).unwrap();
        assert_eq!(state.project.as_ref().unwrap().total_word_count, 9);

        state.split_block_at_cursor(0, "汉字".len(), now()).unwrap();
        assert_eq!(state.current_chapter.as_ref().unwrap().blocks.len(), 3);
        assert!(!state.dirty);
    }

    #[test]
    fn click_editor_outside_clears_focus_and_multi_select() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("块外点击", now()).unwrap();
        state.block_focus = BlockFocus::Editing { index: 0 };
        state.set_multi_select(0, 1);
        state.click_editor_outside();
        assert_eq!(state.block_focus, BlockFocus::Idle);
        assert!(state.block_multi_select.is_none());
    }

    #[test]
    fn switching_chapter_flushes_dirty() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("切换", now()).unwrap();
        state
            .create_chapter_under("", "ch-001", "一", now())
            .unwrap();
        state
            .create_chapter_under("", "ch-002", "二", now())
            .unwrap();
        state.select_chapter("ch-001.json", now()).unwrap();
        state
            .set_block_content_at(0, "甲乙".into(), now(), 0)
            .unwrap();
        assert!(state.dirty);
        state.select_chapter("ch-002.json", now()).unwrap();
        assert!(!state.dirty);
        let path = resolve_rel(state.project_dir.as_ref().unwrap(), "ch-001.json").unwrap();
        let loaded = load_chapter(&path).unwrap();
        assert_eq!(loaded.blocks[0].content, "甲乙");
    }

    #[test]
    fn outline_crud_rename_and_debounced_field_save() {
        use crate::models::OutlineCategory;
        use crate::storage::load_outline_entry;

        let (_dir, mut state) = state_with_temp_root();
        state.new_project("大纲状态", now()).unwrap();
        let id = state
            .create_outline("张三", OutlineCategory::Character, now())
            .unwrap();
        assert_eq!(state.outline_entries.len(), 1);
        assert_eq!(state.current_outline.as_ref().unwrap().key, "张三");

        state.add_outline_field("年龄", now()).unwrap();
        state
            .set_outline_field("年龄", "18".into(), now(), 0)
            .unwrap();
        assert!(state.dirty);
        state.tick_autosave(500, now()).unwrap();
        assert!(!state.dirty);

        let path = outline_entry_path(
            state.project_dir.as_ref().unwrap(),
            OutlineCategory::Character,
            "张三",
        )
        .unwrap();
        let loaded = load_outline_entry(&path).unwrap();
        assert_eq!(loaded.fields.get("年龄").map(String::as_str), Some("18"));

        state.rename_outline(id, "李四", now()).unwrap();
        assert_eq!(state.current_outline.as_ref().unwrap().key, "李四");
        assert!(!path.exists());

        state.delete_outline(id, now()).unwrap();
        assert!(state.outline_entries.is_empty());
        assert!(state.current_outline.is_none());
    }

    #[test]
    fn open_full_text_hit_selects_block() {
        use crate::models::BlockType;
        use crate::services::SearchMode;

        let (_dir, mut state) = state_with_temp_root();
        state.new_project("跳转", now()).unwrap();
        state
            .create_chapter_under("", "ch-001", "命中章", now())
            .unwrap();
        state
            .set_block_content_at(0, "寻找关键词所在".into(), now(), 0)
            .unwrap();
        state.insert_block_at(1, BlockType::Note, now()).unwrap();
        state
            .set_block_content_at(1, "备注也有关键词".into(), now(), 1000)
            .unwrap();
        state.save_manual(now()).unwrap();

        state.set_search_mode(SearchMode::FullText).unwrap();
        state.set_search_query("关键词").unwrap();
        assert_eq!(state.full_text_hits.len(), 2);

        state.open_full_text_hit(1, now()).unwrap();
        assert_eq!(state.block_focus, BlockFocus::Selected { index: 1 });
        assert_eq!(
            state.current_chapter.as_ref().unwrap().blocks[1].content,
            "备注也有关键词"
        );
    }

    #[test]
    fn settings_update_projects_root_and_max_depth() {
        let (_dir, mut state) = state_with_temp_root();
        state.new_project("设置", now()).unwrap();
        let new_root = state
            .config_path
            .parent()
            .unwrap()
            .join("other-novels")
            .to_string_lossy()
            .into_owned();
        state.set_projects_root(&new_root).unwrap();
        let reloaded = AppState::load_from(&state.config_path).unwrap();
        assert_eq!(reloaded.config.projects_root, new_root);

        state.set_max_depth(5, now()).unwrap();
        assert_eq!(state.project.as_ref().unwrap().settings.max_depth, 5);
        let loaded = load_project(state.project_dir.as_ref().unwrap()).unwrap();
        assert_eq!(loaded.settings.max_depth, 5);
    }
}
