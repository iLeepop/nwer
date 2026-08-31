//! 剧本目录树：扫描、CRUD、复制、移动与重排。

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::Script;

use super::script_store::{load_script, save_script};
use super::path_validation::validate_storage_name;
use super::project_store::{RuleViolation, check_can_create_chapter, check_can_create_directory};

/// 相对 `scripts/` 的路径，使用 `/` 分隔（空字符串表示根）。
pub type RelPath = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptNodeKind {
    Directory,
    Script,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptTreeNode {
    /// 相对 `scripts/` 的路径（剧本含 `.json`）。
    pub rel_path: RelPath,
    /// 目录名为文件夹名；剧本为去掉 `.json` 的文件名。
    pub name: String,
    pub kind: ScriptNodeKind,
    pub script_id: Option<Uuid>,
    pub title: Option<String>,
    pub children: Vec<ScriptTreeNode>,
}

impl ScriptTreeNode {
    pub fn is_directory(&self) -> bool {
        self.kind == ScriptNodeKind::Directory
    }

    pub fn is_script(&self) -> bool {
        self.kind == ScriptNodeKind::Script
    }
}

use super::chapter_tree::MoveDirection;

pub fn scripts_dir(project_dir: &Path) -> PathBuf {
    project_dir.join("scripts")
}

/// 将相对路径解析为绝对路径（禁止 `..` 逃逸）。
pub fn resolve_rel(project_dir: &Path, rel: &str) -> Result<PathBuf> {
    let root = scripts_dir(project_dir);
    if rel.is_empty() {
        return Ok(root);
    }
    for seg in rel.split('/') {
        validate_storage_name(seg).with_context(|| format!("invalid path segment `{seg}`"))?;
    }
    let path = rel.split('/').fold(root.clone(), |p, s| p.join(s));
    if !path.starts_with(&root) {
        bail!("path escapes scripts root: {rel}");
    }
    Ok(path)
}

pub fn scan_script_tree(project_dir: &Path) -> Result<Vec<ScriptTreeNode>> {
    let root = scripts_dir(project_dir);
    if !root.exists() {
        return Ok(Vec::new());
    }
    scan_dir(&root, "")
}

fn scan_dir(abs: &Path, rel: &str) -> Result<Vec<ScriptTreeNode>> {
    let mut entries: Vec<_> = fs::read_dir(abs)
        .with_context(|| format!("failed to read {}", abs.display()))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read entry under {}", abs.display()))?;

    entries.sort_by_key(|e| e.file_name());

    let mut nodes = Vec::new();
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let child_rel = if rel.is_empty() {
            name.to_string()
        } else {
            format!("{rel}/{name}")
        };
        let ft = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", entry.path().display()))?;

        if ft.is_dir() {
            let children = scan_dir(&entry.path(), &child_rel)?;
            nodes.push(ScriptTreeNode {
                rel_path: child_rel,
                name: name.into_owned(),
                kind: ScriptNodeKind::Directory,
                script_id: None,
                title: None,
                children,
            });
        } else if ft.is_file() && name.ends_with(".json") {
            let script = load_script(&entry.path())?;
            let stem = name.trim_end_matches(".json").to_string();
            nodes.push(ScriptTreeNode {
                rel_path: child_rel,
                name: stem,
                kind: ScriptNodeKind::Script,
                script_id: Some(script.id),
                title: Some(script.title),
                children: Vec::new(),
            });
        }
    }
    Ok(nodes)
}

/// 在 `parent_rel`（空=根）下新建子目录。
pub fn create_directory(
    project_dir: &Path,
    parent_rel: &str,
    name: &str,
    max_depth: u32,
) -> Result<RelPath> {
    validate_storage_name(name)?;
    let parent = resolve_rel(project_dir, parent_rel)?;
    let chapters_root = scripts_dir(project_dir);
    check_can_create_directory(&chapters_root, &parent, max_depth)?;

    let dest = parent.join(name);
    if dest.exists() {
        bail!("directory already exists: {}", dest.display());
    }
    fs::create_dir_all(&dest)
        .with_context(|| format!("failed to create directory {}", dest.display()))?;

    Ok(rel_join(parent_rel, name))
}

/// 在 `parent_rel` 下新建剧本文件 `{name}.json`。
pub fn create_script_file(
    project_dir: &Path,
    parent_rel: &str,
    name: &str,
    script: &Script,
    max_depth: u32,
) -> Result<RelPath> {
    validate_storage_name(name)?;
    let parent = resolve_rel(project_dir, parent_rel)?;
    let chapters_root = scripts_dir(project_dir);
    check_can_create_chapter(&chapters_root, &parent, max_depth)?;

    let file_name = format!("{name}.json");
    let dest = parent.join(&file_name);
    if dest.exists() {
        bail!("script already exists: {}", dest.display());
    }
    if !parent.exists() {
        fs::create_dir_all(&parent)
            .with_context(|| format!("failed to create parent {}", parent.display()))?;
    }
    save_script(&dest, script)?;
    Ok(rel_join(parent_rel, &file_name))
}

/// 重命名节点（目录或章节）。返回新的相对路径。
pub fn rename_node(project_dir: &Path, rel_path: &str, new_name: &str) -> Result<RelPath> {
    if rel_path.is_empty() {
        bail!("cannot rename scripts root");
    }
    validate_storage_name(new_name)?;
    let src = resolve_rel(project_dir, rel_path)?;
    if !src.exists() {
        bail!("node not found: {rel_path}");
    }

    let parent_rel = parent_rel(rel_path);
    let new_base = if src.is_dir() {
        new_name.to_string()
    } else if rel_path.ends_with(".json") {
        format!("{new_name}.json")
    } else {
        new_name.to_string()
    };

    let dest = resolve_rel(project_dir, parent_rel)?.join(&new_base);
    if dest.exists() {
        bail!("target already exists: {}", dest.display());
    }
    fs::rename(&src, &dest)
        .with_context(|| format!("failed to rename {} -> {}", src.display(), dest.display()))?;
    Ok(rel_join(parent_rel, &new_base))
}

/// 删除节点。目录使用 `remove_dir_all`（含所有后代）。
pub fn delete_node(project_dir: &Path, rel_path: &str) -> Result<()> {
    if rel_path.is_empty() {
        bail!("cannot delete scripts root");
    }
    let path = resolve_rel(project_dir, rel_path)?;
    if !path.exists() {
        bail!("node not found: {rel_path}");
    }
    if path.is_dir() {
        fs::remove_dir_all(&path)
            .with_context(|| format!("failed to delete directory {}", path.display()))?;
    } else {
        fs::remove_file(&path)
            .with_context(|| format!("failed to delete file {}", path.display()))?;
    }
    Ok(())
}

/// 目录是否包含任何非隐藏子项。
pub fn is_nonempty_directory(project_dir: &Path, rel_path: &str) -> Result<bool> {
    let path = resolve_rel(project_dir, rel_path)?;
    if !path.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(&path)? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with('.') {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 复制章节到目标目录，生成新 id。
pub fn copy_script(
    project_dir: &Path,
    src_rel: &str,
    dest_parent_rel: &str,
    new_name: &str,
    max_depth: u32,
    now: DateTime<Utc>,
) -> Result<(RelPath, Script)> {
    let src = resolve_rel(project_dir, src_rel)?;
    if !src.is_file() {
        bail!("copy source must be a script file: {src_rel}");
    }
    let mut script = load_script(&src)?;
    script.id = Uuid::now_v7();
    let _ = now;
    let rel = create_script_file(project_dir, dest_parent_rel, new_name, &script, max_depth)?;
    Ok((rel, script))
}

/// 校验能否将 `src_rel` 移动到 `dest_parent_rel` 下。
pub fn check_can_move(
    project_dir: &Path,
    src_rel: &str,
    dest_parent_rel: &str,
    max_depth: u32,
) -> Result<(), RuleViolation> {
    if src_rel.is_empty() {
        return Err(RuleViolation::OutsideChaptersRoot);
    }
    let chapters_root = scripts_dir(project_dir);
    let src = resolve_rel(project_dir, src_rel).map_err(|_| RuleViolation::OutsideChaptersRoot)?;
    let dest_parent = resolve_rel(project_dir, dest_parent_rel)
        .map_err(|_| RuleViolation::OutsideChaptersRoot)?;

    // 禁止移入自身或后代
    if dest_parent == src || dest_parent.starts_with(&src) {
        return Err(RuleViolation::OutsideChaptersRoot);
    }

    // 同目录（仅重排）总是允许
    if src.parent() == Some(dest_parent.as_path()) {
        return Ok(());
    }

    let height = subtree_height(&src).map_err(RuleViolation::Io)?;
    let rel = dest_parent
        .strip_prefix(&chapters_root)
        .map_err(|_| RuleViolation::OutsideChaptersRoot)?;
    let parent_depth = rel.components().count() as u32;
    let attempted = parent_depth + height;
    if attempted > max_depth {
        return Err(RuleViolation::MaxDepthExceeded {
            attempted,
            max_depth,
        });
    }

    if !dest_parent.exists() {
        return Ok(());
    }

    let (has_dirs, has_scripts) =
        classify_children_simple(&dest_parent).map_err(RuleViolation::Io)?;
    if src.is_dir() {
        if has_scripts {
            Err(RuleViolation::MixedChildren)
        } else {
            Ok(())
        }
    } else if has_dirs {
        Err(RuleViolation::MixedChildren)
    } else {
        Ok(())
    }
}

fn classify_children_simple(dir: &Path) -> Result<(bool, bool), std::io::ErrorKind> {
    let mut has_dirs = false;
    let mut has_scripts = false;
    for entry in fs::read_dir(dir).map_err(|e| e.kind())? {
        let entry = entry.map_err(|e| e.kind())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let ft = entry.file_type().map_err(|e| e.kind())?;
        if ft.is_dir() {
            has_dirs = true;
        } else if ft.is_file() && name.ends_with(".json") {
            has_scripts = true;
        }
        if has_dirs && has_scripts {
            break;
        }
    }
    Ok((has_dirs, has_scripts))
}

/// 子树高度：文件=1，空目录=1，非空=1+max(child)。
fn subtree_height(path: &Path) -> Result<u32, std::io::ErrorKind> {
    if path.is_file() {
        return Ok(1);
    }
    let mut max_child = 0u32;
    for entry in fs::read_dir(path).map_err(|e| e.kind())? {
        let entry = entry.map_err(|e| e.kind())?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let h = subtree_height(&entry.path())?;
        max_child = max_child.max(h);
    }
    Ok(1 + max_child)
}

/// 将节点移动到目标目录下（保留原名，或可选改名）。
pub fn move_node(
    project_dir: &Path,
    src_rel: &str,
    dest_parent_rel: &str,
    new_name: Option<&str>,
    max_depth: u32,
) -> Result<RelPath> {
    check_can_move(project_dir, src_rel, dest_parent_rel, max_depth)?;

    let src = resolve_rel(project_dir, src_rel)?;
    let dest_parent = resolve_rel(project_dir, dest_parent_rel)?;
    if !dest_parent.exists() {
        fs::create_dir_all(&dest_parent)
            .with_context(|| format!("failed to create {}", dest_parent.display()))?;
    }

    let base_name = match new_name {
        Some(n) => {
            validate_storage_name(n)?;
            if src.is_file() {
                format!("{n}.json")
            } else {
                n.to_string()
            }
        }
        None => src
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .context("missing file name")?,
    };

    let dest = dest_parent.join(&base_name);
    if dest.exists() {
        bail!("destination already exists: {}", dest.display());
    }
    fs::rename(&src, &dest)
        .with_context(|| format!("failed to move {} -> {}", src.display(), dest.display()))?;
    Ok(rel_join(dest_parent_rel, &base_name))
}

/// 同目录上移/下移（通过临时名交换相邻兄弟）。
pub fn move_sibling(
    project_dir: &Path,
    rel_path: &str,
    direction: MoveDirection,
) -> Result<RelPath> {
    if rel_path.is_empty() {
        bail!("cannot move scripts root");
    }
    let parent_rel_str = parent_rel(rel_path).to_string();
    let parent = resolve_rel(project_dir, &parent_rel_str)?;
    let siblings = list_sibling_names(&parent)?;
    let base = rel_path
        .rsplit('/')
        .next()
        .context("empty rel_path")?
        .to_string();
    let idx = siblings
        .iter()
        .position(|s| s == &base)
        .with_context(|| format!("sibling not found: {base}"))?;
    let swap_idx = match direction {
        MoveDirection::Up => {
            if idx == 0 {
                return Ok(rel_path.to_string());
            }
            idx - 1
        }
        MoveDirection::Down => {
            if idx + 1 >= siblings.len() {
                return Ok(rel_path.to_string());
            }
            idx + 1
        }
    };

    let name_a = siblings[idx].clone();
    let name_b = siblings[swap_idx].clone();
    let path_a = parent.join(&name_a);
    let path_b = parent.join(&name_b);
    let tmp = parent.join(format!(".__nwer_swap_{name_a}"));
    fs::rename(&path_a, &tmp)?;
    fs::rename(&path_b, &path_a)?;
    fs::rename(&tmp, &path_b)?;

    // After swap, our node has name_b's former path name... wait:
    // We renamed A->tmp, B->A's path (so B content is now at name_a), tmp->B's path (A content at name_b).
    // So our original A content is now at name_b.
    Ok(rel_join(&parent_rel_str, &name_b))
}

fn list_sibling_names(parent: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() || (ft.is_file() && name.ends_with(".json")) {
            names.push(name.into_owned());
        }
    }
    names.sort();
    Ok(names)
}

pub fn find_node_by_script_id(nodes: &[ScriptTreeNode], id: Uuid) -> Option<&ScriptTreeNode> {
    for n in nodes {
        if n.script_id == Some(id) {
            return Some(n);
        }
        if let Some(found) = find_node_by_script_id(&n.children, id) {
            return Some(found);
        }
    }
    None
}

pub fn find_node_by_rel<'a>(
    nodes: &'a [ScriptTreeNode],
    rel: &str,
) -> Option<&'a ScriptTreeNode> {
    for n in nodes {
        if n.rel_path == rel {
            return Some(n);
        }
        if let Some(found) = find_node_by_rel(&n.children, rel) {
            return Some(found);
        }
    }
    None
}

fn parent_rel(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    }
}

fn rel_join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    use crate::storage::project_store::create_project;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 30, 1, 0, 0).unwrap()
    }

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let root = tempdir().unwrap();
        let (project_dir, _) = create_project(root.path(), "树测试", now()).unwrap();
        (root, project_dir)
    }

    #[test]
    fn scan_empty_chapters_returns_empty() {
        let (_t, project_dir) = setup();
        let tree = scan_script_tree(&project_dir).unwrap();
        assert!(tree.is_empty());
    }

    #[test]
    fn create_directory_and_chapter_respects_mutual_exclusion() {
        let (_t, project_dir) = setup();
        let max = 3u32;

        let vol = create_directory(&project_dir, "", "vol-001第一卷", max).unwrap();
        assert_eq!(vol, "vol-001第一卷");

        let err = create_script_file(
            &project_dir,
            "",
            "ch-001开篇",
            &Script::new("开篇", now()),
            max,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("mix") || err.to_string().contains("Mixed") || {
                // RuleViolation via anyhow
                format!("{err:#}").contains("mix")
            }
        );

        let part = create_directory(&project_dir, &vol, "part-001上篇", max).unwrap();
        let ch = create_script_file(
            &project_dir,
            &part,
            "ch-001开篇",
            &Script::new("开篇", now()),
            max,
        )
        .unwrap();
        assert_eq!(ch, "vol-001第一卷/part-001上篇/ch-001开篇.json");

        let tree = scan_script_tree(&project_dir).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "vol-001第一卷");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].children.len(), 1);
        assert_eq!(
            tree[0].children[0].children[0].title.as_deref(),
            Some("开篇")
        );
    }

    #[test]
    fn create_rejects_depth_overflow() {
        let (_t, project_dir) = setup();
        let max = 3u32;
        create_directory(&project_dir, "", "a", max).unwrap();
        create_directory(&project_dir, "a", "b", max).unwrap();
        create_directory(&project_dir, "a/b", "c", max).unwrap();
        let err = create_directory(&project_dir, "a/b/c", "d", max).unwrap_err();
        assert!(
            format!("{err:#}").contains("max_depth") || format!("{err:#}").contains("depth"),
            "{err:#}"
        );
    }

    #[test]
    fn rename_and_delete_directory_with_descendants() {
        let (_t, project_dir) = setup();
        let max = 3u32;
        create_directory(&project_dir, "", "vol-001", max).unwrap();
        create_script_file(
            &project_dir,
            "vol-001",
            "ch-001",
            &Script::new("一", now()),
            max,
        )
        .unwrap();

        assert!(is_nonempty_directory(&project_dir, "vol-001").unwrap());

        let renamed = rename_node(&project_dir, "vol-001", "vol-002卷二").unwrap();
        assert_eq!(renamed, "vol-002卷二");
        assert!(
            resolve_rel(&project_dir, "vol-002卷二/ch-001.json")
                .unwrap()
                .is_file()
        );

        delete_node(&project_dir, "vol-002卷二").unwrap();
        let tree = scan_script_tree(&project_dir).unwrap();
        assert!(tree.is_empty());
    }

    #[test]
    fn copy_script_creates_new_id() {
        let (_t, project_dir) = setup();
        let max = 3u32;
        let ch = Script::new("原稿", now());
        let src_id = ch.id;
        create_script_file(&project_dir, "", "ch-001原稿", &ch, max).unwrap();

        let (new_rel, copied) = copy_script(
            &project_dir,
            "ch-001原稿.json",
            "",
            "ch-002副本",
            max,
            now(),
        )
        .unwrap();
        assert_eq!(new_rel, "ch-002副本.json");
        assert_ne!(copied.id, src_id);
        assert_eq!(copied.title, "原稿");
        assert_eq!(copied.blocks.len(), ch.blocks.len());
    }

    #[test]
    fn move_chapter_across_dirs_and_reject_mixed() {
        let (_t, project_dir) = setup();
        let max = 3u32;
        create_directory(&project_dir, "", "vol-a", max).unwrap();
        create_directory(&project_dir, "", "vol-b", max).unwrap();
        create_script_file(
            &project_dir,
            "vol-a",
            "ch-001",
            &Script::new("甲", now()),
            max,
        )
        .unwrap();

        let moved = move_node(&project_dir, "vol-a/ch-001.json", "vol-b", None, max).unwrap();
        assert_eq!(moved, "vol-b/ch-001.json");
        assert!(
            !resolve_rel(&project_dir, "vol-a/ch-001.json")
                .unwrap()
                .exists()
        );
        assert!(
            resolve_rel(&project_dir, "vol-b/ch-001.json")
                .unwrap()
                .is_file()
        );

        // vol-a 现为空目录；在根放章节会与 vol-b 目录互斥
        let err = move_node(&project_dir, "vol-b/ch-001.json", "", None, max);
        assert!(err.is_err(), "should reject script next to directories");
    }

    #[test]
    fn move_rejects_depth_exceeded_for_subtree() {
        let (_t, project_dir) = setup();
        let max = 3u32;
        // scripts/a/b/ch.json  height of a = 3
        create_directory(&project_dir, "", "a", max).unwrap();
        create_directory(&project_dir, "a", "b", max).unwrap();
        create_script_file(
            &project_dir,
            "a/b",
            "ch-001",
            &Script::new("深", now()),
            max,
        )
        .unwrap();
        create_directory(&project_dir, "", "x", max).unwrap();
        create_directory(&project_dir, "x", "y", max).unwrap();
        // move a under x/y → x/y/a/b/ch depth 5 > max 3
        let err = check_can_move(&project_dir, "a", "x/y", max).unwrap_err();
        assert_eq!(
            err,
            RuleViolation::MaxDepthExceeded {
                attempted: 5,
                max_depth: 3
            }
        );
    }

    #[test]
    fn move_sibling_up_down_swaps_order() {
        let (_t, project_dir) = setup();
        let max = 3u32;
        create_script_file(&project_dir, "", "ch-001", &Script::new("一", now()), max).unwrap();
        create_script_file(&project_dir, "", "ch-002", &Script::new("二", now()), max).unwrap();

        let after = move_sibling(&project_dir, "ch-002.json", MoveDirection::Up).unwrap();
        assert_eq!(after, "ch-001.json"); // content of former ch-002 now at ch-001 name

        // After swap, names are swapped: file named ch-001.json has title 二, ch-002 has title 一
        let tree = scan_script_tree(&project_dir).unwrap();
        assert_eq!(tree[0].name, "ch-001");
        assert_eq!(tree[0].title.as_deref(), Some("二"));
        assert_eq!(tree[1].title.as_deref(), Some("一"));

        move_sibling(&project_dir, "ch-001.json", MoveDirection::Down).unwrap();
        let tree = scan_script_tree(&project_dir).unwrap();
        assert_eq!(tree[0].title.as_deref(), Some("一"));
        assert_eq!(tree[1].title.as_deref(), Some("二"));
    }

    #[test]
    fn rename_chapter_file() {
        let (_t, project_dir) = setup();
        create_script_file(&project_dir, "", "ch-001旧", &Script::new("旧", now()), 3).unwrap();
        let new_rel = rename_node(&project_dir, "ch-001旧.json", "ch-002新").unwrap();
        assert_eq!(new_rel, "ch-002新.json");
        let ch = load_script(&resolve_rel(&project_dir, &new_rel).unwrap()).unwrap();
        assert_eq!(ch.title, "旧");
    }
}
