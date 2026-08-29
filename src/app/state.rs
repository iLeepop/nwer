use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};

use crate::models::Project;
use crate::storage::{
    AppConfig, add_recent_project, create_project, load_config_from, load_project, save_config_to,
};

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

/// 工作区 UI 会话状态（阶段 1 骨架）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceUi {
    pub sidebar_tab: SidebarTab,
    pub ai_panel_open: bool,
    pub sidebar_visible: bool,
}

impl Default for WorkspaceUi {
    fn default() -> Self {
        Self {
            sidebar_tab: SidebarTab::Chapters,
            ai_panel_open: false,
            sidebar_visible: true,
        }
    }
}

/// 应用协调状态：当前项目、配置、dirty 与 UI。
#[derive(Debug, Clone)]
pub struct AppState {
    pub project: Option<Project>,
    pub project_dir: Option<PathBuf>,
    pub config: AppConfig,
    /// 配置文件路径（测试可注入临时路径）。
    pub config_path: PathBuf,
    pub ui: WorkspaceUi,
    pub dirty: bool,
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
        })
    }

    /// 展开 `projects_root` 中的 `~`。
    pub fn expanded_projects_root(&self) -> Result<PathBuf> {
        expand_user_path(&self.config.projects_root)
    }

    /// 在 projects_root 下新建项目并记入最近列表。
    pub fn new_project(&mut self, title: impl Into<String>, now: DateTime<Utc>) -> Result<()> {
        let title = title.into();
        let root = self.expanded_projects_root()?;
        let (project_dir, project) = create_project(&root, &title, now)?;
        self.set_current_project(project_dir, project, now)?;
        Ok(())
    }

    /// 打开已有项目目录并记入最近列表。
    pub fn open_project(&mut self, path: impl AsRef<Path>, now: DateTime<Utc>) -> Result<()> {
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
    }

    pub fn set_sidebar_tab(&mut self, tab: SidebarTab) {
        self.ui.sidebar_tab = tab;
    }

    pub fn current_title(&self) -> &str {
        self.project
            .as_ref()
            .map(|p| p.title.as_str())
            .unwrap_or("未打开项目")
    }

    fn set_current_project(
        &mut self,
        project_dir: PathBuf,
        project: Project,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.ui.sidebar_tab = SidebarTab::parse(&project.ui_state.sidebar_tab);
        self.ui.ai_panel_open = project.ui_state.ai_panel_open;

        let path_str = project_dir.to_string_lossy().into_owned();
        add_recent_project(&mut self.config, path_str, project.title.clone(), now);
        save_config_to(&self.config_path, &self.config)?;

        self.project_dir = Some(project_dir);
        self.project = Some(project);
        self.dirty = false;
        Ok(())
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

        // 模拟关闭：丢弃内存态，再从磁盘打开
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
}
