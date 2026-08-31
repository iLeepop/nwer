use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{OutlineCategory, Project};

use super::atomic_write;
use super::path_validation::validate_storage_name;

/// 章节树放置规则违规。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleViolation {
    /// 同一目录不能同时包含子目录和章节 JSON。
    MixedChildren,
    /// 超出项目 `max_depth`。
    MaxDepthExceeded { attempted: u32, max_depth: u32 },
    /// 父路径不在 `chapters/` 下。
    OutsideChaptersRoot,
    /// 读取目录内容时发生 IO 错误。
    Io(std::io::ErrorKind),
}

impl std::fmt::Display for RuleViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleViolation::MixedChildren => {
                write!(f, "directory cannot mix subdirectories and chapter files")
            }
            RuleViolation::MaxDepthExceeded {
                attempted,
                max_depth,
            } => {
                write!(f, "depth {attempted} exceeds max_depth {max_depth}")
            }
            RuleViolation::OutsideChaptersRoot => {
                write!(f, "path is outside the chapters root")
            }
            RuleViolation::Io(kind) => {
                write!(f, "failed to inspect directory: {kind}")
            }
        }
    }
}

impl std::error::Error for RuleViolation {}

/// 最近打开的项目记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: String,
    pub title: String,
    pub last_opened: DateTime<Utc>,
}

/// 全局 AI 连接设置（所有项目共用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiSettings {
    /// snake_case：`deepseek` | `kimi` | `ollama` | `vllm` | `local`
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    /// 自由文本模型 id
    pub model: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: "deepseek".into(),
            api_key: String::new(),
            base_url: default_base_url_for_provider("deepseek")
                .unwrap_or_else(|| "https://api.deepseek.com".into()),
            model: String::new(),
        }
    }
}

/// 提供商列表（UI 下拉用）：`(id, 显示名)`。
pub fn ai_providers() -> &'static [(&'static str, &'static str)] {
    &[
        ("deepseek", "DeepSeek"),
        ("kimi", "Kimi"),
        ("ollama", "Ollama"),
        ("vllm", "vLLM"),
        ("local", "Local"),
    ]
}

/// 切换提供商时写入的默认调用地址。
pub fn default_base_url_for_provider(provider: &str) -> Option<String> {
    let url = match provider {
        "deepseek" => "https://api.deepseek.com",
        "kimi" => "https://api.moonshot.cn/v1",
        "ollama" => "http://127.0.0.1:11434",
        "vllm" => "http://127.0.0.1:8000",
        "local" => "http://127.0.0.1:8080",
        _ => return None,
    };
    Some(url.to_string())
}

/// 设置保存校验：`projects_root` 必填；非空 `base_url` 须以 http(s) 开头。
pub fn validate_settings_save(projects_root: &str, base_url: &str) -> Result<()> {
    if projects_root.trim().is_empty() {
        bail!("projects_root must not be empty");
    }
    let url = base_url.trim();
    if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
        bail!("base_url must start with http:// or https://");
    }
    Ok(())
}

/// 应用配置（平台配置目录下 `nwer/config.json`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    pub projects_root: String,
    #[serde(default)]
    pub recent_projects: Vec<RecentProject>,
    #[serde(default = "default_max_recent")]
    pub max_recent: usize,
    #[serde(default)]
    pub ai: AiSettings,
}

fn default_max_recent() -> usize {
    10
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            projects_root: "~/Documents/Novels".to_string(),
            recent_projects: Vec::new(),
            max_recent: default_max_recent(),
            ai: AiSettings::default(),
        }
    }
}

/// 平台配置目录下的 `nwer/config.json` 路径。
pub fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("cannot resolve platform config directory")?;
    Ok(base.join("nwer").join("config.json"))
}

pub fn load_config() -> Result<AppConfig> {
    load_config_from(&config_path()?)
}

pub fn save_config(config: &AppConfig) -> Result<()> {
    save_config_to(&config_path()?, config)
}

pub fn load_config_from(path: &Path) -> Result<AppConfig> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    let config: AppConfig = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse config {}", path.display()))?;
    Ok(config)
}

pub fn save_config_to(path: &Path, config: &AppConfig) -> Result<()> {
    let json = serde_json::to_string_pretty(config).context("failed to serialize config")?;
    atomic_write(path, json.as_bytes())
        .with_context(|| format!("failed to write config {}", path.display()))?;
    Ok(())
}

/// 将项目记入最近列表（按 `last_opened` 倒序，最多 `max_recent` 条）。
/// 同路径已存在时更新并前置；不删除磁盘内容。
pub fn add_recent_project(
    config: &mut AppConfig,
    path: impl Into<String>,
    title: impl Into<String>,
    last_opened: DateTime<Utc>,
) {
    let path = path.into();
    config.recent_projects.retain(|r| r.path != path);
    config.recent_projects.push(RecentProject {
        path,
        title: title.into(),
        last_opened,
    });
    config
        .recent_projects
        .sort_by_key(|r| std::cmp::Reverse(r.last_opened));
    let limit = config.max_recent.max(1);
    config.recent_projects.truncate(limit);
}

/// 从最近列表移除；不删除磁盘项目。
pub fn remove_recent_project(config: &mut AppConfig, path: &str) {
    config.recent_projects.retain(|r| r.path != path);
}

/// 在 `projects_root` 下创建新项目目录结构并写入 `project.json`。
pub fn create_project(
    projects_root: &Path,
    title: impl Into<String>,
    now: DateTime<Utc>,
) -> Result<(PathBuf, Project)> {
    let title = title.into();
    validate_storage_name(&title)?;

    fs::create_dir_all(projects_root)
        .with_context(|| format!("failed to create projects root {}", projects_root.display()))?;

    let project_dir = projects_root.join(&title);
    if project_dir.exists() {
        bail!("project already exists at {}", project_dir.display());
    }

    fs::create_dir_all(project_dir.join("chapters")).with_context(|| {
        format!(
            "failed to create chapters dir under {}",
            project_dir.display()
        )
    })?;

    fs::create_dir_all(project_dir.join("scripts")).with_context(|| {
        format!(
            "failed to create scripts dir under {}",
            project_dir.display()
        )
    })?;

    for category in OutlineCategory::all() {
        let dir = project_dir.join("outline").join(category.label());
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create outline dir {}", dir.display()))?;
    }

    let project = Project::new(title, now);
    save_project(&project_dir, &project)?;
    Ok((project_dir, project))
}

pub fn load_project(project_dir: &Path) -> Result<Project> {
    let path = project_dir.join("project.json");
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let project: Project = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(project)
}

pub fn save_project(project_dir: &Path, project: &Project) -> Result<()> {
    let path = project_dir.join("project.json");
    let json = serde_json::to_string_pretty(project).context("failed to serialize project")?;
    atomic_write(&path, format!("{json}\n").as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// 校验是否可在 `parent_dir` 下新建子目录。
pub fn check_can_create_directory(
    chapters_root: &Path,
    parent_dir: &Path,
    max_depth: u32,
) -> Result<(), RuleViolation> {
    check_placement(
        chapters_root,
        parent_dir,
        max_depth,
        PlacementKind::Directory,
    )
}

/// 校验是否可在 `parent_dir` 下新建章节 JSON。
pub fn check_can_create_chapter(
    chapters_root: &Path,
    parent_dir: &Path,
    max_depth: u32,
) -> Result<(), RuleViolation> {
    check_placement(chapters_root, parent_dir, max_depth, PlacementKind::Chapter)
}

#[derive(Clone, Copy)]
enum PlacementKind {
    Directory,
    Chapter,
}

fn check_placement(
    chapters_root: &Path,
    parent_dir: &Path,
    max_depth: u32,
    kind: PlacementKind,
) -> Result<(), RuleViolation> {
    let rel = parent_dir
        .strip_prefix(chapters_root)
        .map_err(|_| RuleViolation::OutsideChaptersRoot)?;

    let parent_depth = rel.components().count() as u32;
    let attempted = parent_depth + 1;
    if attempted > max_depth {
        return Err(RuleViolation::MaxDepthExceeded {
            attempted,
            max_depth,
        });
    }

    if !parent_dir.exists() {
        return Ok(());
    }

    let (has_dirs, has_chapters) = classify_children(parent_dir).map_err(RuleViolation::Io)?;

    match kind {
        PlacementKind::Directory if has_chapters => Err(RuleViolation::MixedChildren),
        PlacementKind::Chapter if has_dirs => Err(RuleViolation::MixedChildren),
        _ => Ok(()),
    }
}

fn classify_children(dir: &Path) -> Result<(bool, bool), std::io::ErrorKind> {
    let mut has_dirs = false;
    let mut has_chapters = false;

    for entry in fs::read_dir(dir).map_err(|e| e.kind())? {
        let entry = entry.map_err(|e| e.kind())?;
        let file_type = entry.file_type().map_err(|e| e.kind())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if name.starts_with('.') {
            continue;
        }

        if file_type.is_dir() {
            has_dirs = true;
        } else if file_type.is_file() && name.ends_with(".json") {
            has_chapters = true;
        }

        if has_dirs && has_chapters {
            break;
        }
    }

    Ok((has_dirs, has_chapters))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap()
    }

    #[test]
    fn create_project_writes_expected_structure_and_reloads() {
        let root = tempdir().unwrap();
        let (project_dir, created) = create_project(root.path(), "示例小说", now()).unwrap();

        assert!(project_dir.join("project.json").is_file());
        assert!(project_dir.join("chapters").is_dir());
        assert!(project_dir.join("scripts").is_dir());
        for label in ["角色", "背景", "场景", "事件", "杂项"] {
            assert!(
                project_dir.join("outline").join(label).is_dir(),
                "missing outline/{label}"
            );
        }

        let loaded = load_project(&project_dir).unwrap();
        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.title, "示例小说");
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.settings.max_depth, 3);
    }

    #[test]
    fn save_project_roundtrips_updates() {
        let root = tempdir().unwrap();
        let (project_dir, mut project) = create_project(root.path(), "往返", now()).unwrap();
        project.total_word_count = 42;
        project.ui_state.sidebar_tab = "outline".into();
        save_project(&project_dir, &project).unwrap();

        let loaded = load_project(&project_dir).unwrap();
        assert_eq!(loaded, project);
    }

    #[test]
    fn create_project_rejects_duplicate_directory() {
        let root = tempdir().unwrap();
        create_project(root.path(), "同名", now()).unwrap();
        let err = create_project(root.path(), "同名", now()).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn create_project_rejects_invalid_title() {
        let root = tempdir().unwrap();
        for title in ["", "  ", "../escape", "foo/bar", ".."] {
            let err = create_project(root.path(), title, now()).unwrap_err();
            assert!(
                !err.to_string().contains("already exists"),
                "title {title:?} should fail validation, not duplicate check"
            );
        }
    }

    #[test]
    fn io_error_from_classify_children_is_not_outside_chapters_root() {
        let root = tempdir().unwrap();
        let chapters = root.path().join("chapters");
        fs::create_dir_all(&chapters).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&chapters, fs::Permissions::from_mode(0o000)).unwrap();
            let err = check_can_create_chapter(&chapters, &chapters, 3).unwrap_err();
            assert!(matches!(err, RuleViolation::Io(_)));
            assert_ne!(err, RuleViolation::OutsideChaptersRoot);
            fs::set_permissions(&chapters, fs::Permissions::from_mode(0o755)).unwrap();
        }

        #[cfg(not(unix))]
        {
            // Non-Unix: use a path under chapters that does not exist as parent
            // (outside root is tested separately); skip permission-based IO test.
            let outside = root.path().join("not-chapters");
            let err = check_can_create_chapter(&chapters, &outside, 3).unwrap_err();
            assert_eq!(err, RuleViolation::OutsideChaptersRoot);
        }
    }

    #[test]
    fn rejects_chapter_when_parent_has_subdirectory() {
        let root = tempdir().unwrap();
        let chapters = root.path().join("chapters");
        fs::create_dir_all(chapters.join("vol-001")).unwrap();

        let err = check_can_create_chapter(&chapters, &chapters, 3).unwrap_err();
        assert_eq!(err, RuleViolation::MixedChildren);
    }

    #[test]
    fn rejects_directory_when_parent_has_chapter_file() {
        let root = tempdir().unwrap();
        let chapters = root.path().join("chapters");
        fs::create_dir_all(&chapters).unwrap();
        fs::write(chapters.join("ch-001开篇.json"), b"{}").unwrap();

        let err = check_can_create_directory(&chapters, &chapters, 3).unwrap_err();
        assert_eq!(err, RuleViolation::MixedChildren);
    }

    #[test]
    fn allows_chapter_among_chapters_and_directory_among_dirs() {
        let root = tempdir().unwrap();
        let chapters = root.path().join("chapters");
        fs::create_dir_all(chapters.join("vol-001")).unwrap();
        check_can_create_directory(&chapters, &chapters, 3).unwrap();

        let leaf = chapters.join("vol-001");
        fs::write(leaf.join("ch-001.json"), b"{}").unwrap();
        check_can_create_chapter(&chapters, &leaf, 3).unwrap();
    }

    #[test]
    fn rejects_when_depth_exceeds_max() {
        let root = tempdir().unwrap();
        let chapters = root.path().join("chapters");
        let deep = chapters.join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();

        // parent depth 3 → new node depth 4 > max 3
        let err = check_can_create_chapter(&chapters, &deep, 3).unwrap_err();
        assert_eq!(
            err,
            RuleViolation::MaxDepthExceeded {
                attempted: 4,
                max_depth: 3
            }
        );

        let err = check_can_create_directory(&chapters, &deep, 3).unwrap_err();
        assert_eq!(
            err,
            RuleViolation::MaxDepthExceeded {
                attempted: 4,
                max_depth: 3
            }
        );
    }

    #[test]
    fn allows_chapter_at_exact_max_depth() {
        let root = tempdir().unwrap();
        let chapters = root.path().join("chapters");
        let parent = chapters.join("a").join("b");
        fs::create_dir_all(&parent).unwrap();
        // depth 3 chapter under depth-2 parent
        check_can_create_chapter(&chapters, &parent, 3).unwrap();
    }

    #[test]
    fn config_roundtrip_and_recent_ordering() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");

        let mut config = AppConfig::default();
        assert_eq!(config.projects_root, "~/Documents/Novels");
        assert_eq!(config.max_recent, 10);

        let t1 = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 8, 29, 10, 0, 0).unwrap();

        add_recent_project(&mut config, "/p/a", "A", t1);
        add_recent_project(&mut config, "/p/b", "B", t2);
        add_recent_project(&mut config, "/p/c", "C", t3);
        assert_eq!(
            config
                .recent_projects
                .iter()
                .map(|r| r.title.as_str())
                .collect::<Vec<_>>(),
            vec!["C", "B", "A"]
        );

        // 再次打开 A，应升到最前
        let t4 = Utc.with_ymd_and_hms(2026, 8, 29, 11, 0, 0).unwrap();
        add_recent_project(&mut config, "/p/a", "A", t4);
        assert_eq!(config.recent_projects.len(), 3);
        assert_eq!(config.recent_projects[0].path, "/p/a");
        assert_eq!(config.recent_projects[0].last_opened, t4);

        remove_recent_project(&mut config, "/p/b");
        assert_eq!(config.recent_projects.len(), 2);
        assert!(config.recent_projects.iter().all(|r| r.path != "/p/b"));

        save_config_to(&path, &config).unwrap();
        let loaded = load_config_from(&path).unwrap();
        assert_eq!(loaded, config);
    }

    #[test]
    fn recent_list_respects_max_recent() {
        let mut config = AppConfig {
            max_recent: 2,
            ..AppConfig::default()
        };
        add_recent_project(
            &mut config,
            "/1",
            "1",
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        );
        add_recent_project(
            &mut config,
            "/2",
            "2",
            Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap(),
        );
        add_recent_project(
            &mut config,
            "/3",
            "3",
            Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap(),
        );
        assert_eq!(config.recent_projects.len(), 2);
        assert_eq!(config.recent_projects[0].path, "/3");
        assert_eq!(config.recent_projects[1].path, "/2");
    }

    #[test]
    fn load_missing_config_returns_defaults() {
        let dir = tempdir().unwrap();
        let loaded = load_config_from(&dir.path().join("missing.json")).unwrap();
        assert_eq!(loaded, AppConfig::default());
    }

    #[test]
    fn legacy_config_without_ai_deserializes_defaults() {
        let json = r#"{
            "projects_root": "~/Novels",
            "recent_projects": [],
            "max_recent": 10
        }"#;
        let loaded: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.projects_root, "~/Novels");
        assert_eq!(loaded.ai, AiSettings::default());
        assert_eq!(loaded.ai.provider, "deepseek");
        assert_eq!(loaded.ai.base_url, "https://api.deepseek.com");
    }

    #[test]
    fn ai_settings_roundtrip_json() {
        let ai = AiSettings {
            provider: "kimi".into(),
            api_key: "sk-test".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            model: "moonshot-v1".into(),
        };
        let json = serde_json::to_string(&ai).unwrap();
        let back: AiSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ai);
    }

    #[test]
    fn default_base_url_for_known_providers() {
        assert_eq!(
            default_base_url_for_provider("deepseek").as_deref(),
            Some("https://api.deepseek.com")
        );
        assert_eq!(
            default_base_url_for_provider("kimi").as_deref(),
            Some("https://api.moonshot.cn/v1")
        );
        assert_eq!(
            default_base_url_for_provider("ollama").as_deref(),
            Some("http://127.0.0.1:11434")
        );
        assert!(default_base_url_for_provider("unknown").is_none());
    }

    #[test]
    fn validate_settings_save_rules() {
        assert!(validate_settings_save("~/x", "").is_ok());
        assert!(validate_settings_save("~/x", "https://api.example.com").is_ok());
        assert!(validate_settings_save("  ", "").is_err());
        assert!(validate_settings_save("~/x", "ftp://bad").is_err());
    }
}
