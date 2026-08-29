use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};

use crate::models::{OutlineCategory, OutlineEntry};

use super::atomic_write;
use super::path_validation::validate_storage_name;

/// 大纲条目默认落盘路径：`outline/<分类>/<key>.json`。
pub fn outline_entry_path(
    project_dir: &Path,
    category: OutlineCategory,
    key: &str,
) -> Result<PathBuf> {
    validate_storage_name(key)?;
    Ok(project_dir
        .join("outline")
        .join(category.label())
        .join(format!("{key}.json")))
}

pub fn load_outline_entry(path: &Path) -> Result<OutlineEntry> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read outline entry {}", path.display()))?;
    let entry: OutlineEntry = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse outline entry {}", path.display()))?;
    Ok(entry)
}

pub fn save_outline_entry(path: &Path, entry: &OutlineEntry) -> Result<()> {
    let json = serde_json::to_string_pretty(entry).context("failed to serialize outline entry")?;
    atomic_write(path, format!("{json}\n").as_bytes())
        .with_context(|| format!("failed to write outline entry {}", path.display()))?;
    Ok(())
}

/// 列出项目下全部大纲条目（按分类顺序，同分类内按文件名排序）。
pub fn list_outline_entries(project_dir: &Path) -> Result<Vec<OutlineEntry>> {
    let mut entries = Vec::new();
    for category in OutlineCategory::all() {
        let dir = project_dir.join("outline").join(category.label());
        if !dir.exists() {
            continue;
        }
        let mut files: Vec<_> = fs::read_dir(&dir)
            .with_context(|| format!("failed to read {}", dir.display()))?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| format!("failed to read entry under {}", dir.display()))?;
        files.sort_by_key(|e| e.file_name());
        for file in files {
            let name = file.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || !name.ends_with(".json") {
                continue;
            }
            let path = file.path();
            if path.is_file() {
                entries.push(load_outline_entry(&path)?);
            }
        }
    }
    Ok(entries)
}

/// 新建空字段大纲条目并落盘。
pub fn create_outline_entry(
    project_dir: &Path,
    key: &str,
    category: OutlineCategory,
    now: DateTime<Utc>,
) -> Result<OutlineEntry> {
    let path = outline_entry_path(project_dir, category, key)?;
    if path.exists() {
        bail!("outline entry already exists: {}", path.display());
    }
    let entry = OutlineEntry::new(key, category, now);
    save_outline_entry(&path, &entry)?;
    Ok(entry)
}

/// 删除大纲条目文件。
pub fn delete_outline_entry(
    project_dir: &Path,
    category: OutlineCategory,
    key: &str,
) -> Result<()> {
    let path = outline_entry_path(project_dir, category, key)?;
    if !path.exists() {
        bail!("outline entry not found: {}", path.display());
    }
    fs::remove_file(&path)
        .with_context(|| format!("failed to delete outline entry {}", path.display()))?;
    Ok(())
}

/// 重命名条目：更新 key 后写入新路径并删除旧文件（同目录原子写入新文件）。
pub fn rename_outline_entry(
    project_dir: &Path,
    category: OutlineCategory,
    old_key: &str,
    new_key: &str,
    now: DateTime<Utc>,
) -> Result<OutlineEntry> {
    if old_key == new_key {
        let path = outline_entry_path(project_dir, category, old_key)?;
        return load_outline_entry(&path);
    }
    let old_path = outline_entry_path(project_dir, category, old_key)?;
    let new_path = outline_entry_path(project_dir, category, new_key)?;
    if !old_path.exists() {
        bail!("outline entry not found: {}", old_path.display());
    }
    if new_path.exists() {
        bail!("outline entry already exists: {}", new_path.display());
    }

    let mut entry = load_outline_entry(&old_path)?;
    entry.key = new_key.to_string();
    entry.meta.updated_at = now;
    save_outline_entry(&new_path, &entry)?;
    fs::remove_file(&old_path).with_context(|| {
        format!(
            "failed to remove old outline entry after rename {}",
            old_path.display()
        )
    })?;
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::storage::project_store::create_project;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap()
    }

    fn project_dir() -> (tempfile::TempDir, PathBuf) {
        let root = tempdir().unwrap();
        let (dir, _) = create_project(root.path(), "大纲项目", now()).unwrap();
        (root, dir)
    }

    #[test]
    fn outline_entry_path_uses_category_label() {
        let path =
            outline_entry_path(Path::new("/novel"), OutlineCategory::Character, "张三").unwrap();
        assert_eq!(path, PathBuf::from("/novel/outline/角色/张三.json"));
    }

    #[test]
    fn outline_entry_path_rejects_invalid_key() {
        for key in ["", "../evil", "foo/bar", ".."] {
            let err = outline_entry_path(Path::new("/novel"), OutlineCategory::Character, key)
                .unwrap_err();
            assert!(
                err.to_string().contains("empty")
                    || err.to_string().contains("..")
                    || err.to_string().contains("path separators"),
                "key {key:?}: {err}"
            );
        }
    }

    #[test]
    fn outline_save_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = outline_entry_path(dir.path(), OutlineCategory::Character, "张三").unwrap();
        let mut entry = OutlineEntry::new("张三", OutlineCategory::Character, now());
        entry.fields.insert("身份".into(), "弟子".into());

        save_outline_entry(&path, &entry).unwrap();
        let loaded = load_outline_entry(&path).unwrap();
        assert_eq!(loaded, entry);
    }

    #[test]
    fn crud_across_five_categories_and_list() {
        let (_root, project_dir) = project_dir();

        for category in OutlineCategory::all() {
            let key = format!("条目-{}", category.label());
            let entry = create_outline_entry(&project_dir, &key, category, now()).unwrap();
            assert_eq!(entry.key, key);
            assert_eq!(entry.category, category);
            assert!(entry.fields.is_empty());
            assert!(
                outline_entry_path(&project_dir, category, &key)
                    .unwrap()
                    .is_file()
            );
        }

        let listed = list_outline_entries(&project_dir).unwrap();
        assert_eq!(listed.len(), 5);
        for category in OutlineCategory::all() {
            assert!(
                listed.iter().any(|e| e.category == category),
                "missing {category:?}"
            );
        }

        // update fields
        let mut entry = listed
            .into_iter()
            .find(|e| e.category == OutlineCategory::Character)
            .unwrap();
        entry.fields.insert("年龄".into(), "18".into());
        entry.meta.updated_at = now();
        save_outline_entry(
            &outline_entry_path(&project_dir, entry.category, &entry.key).unwrap(),
            &entry,
        )
        .unwrap();
        let reloaded = load_outline_entry(
            &outline_entry_path(&project_dir, OutlineCategory::Character, &entry.key).unwrap(),
        )
        .unwrap();
        assert_eq!(reloaded.fields.get("年龄").map(String::as_str), Some("18"));

        delete_outline_entry(&project_dir, OutlineCategory::Misc, "条目-杂项").unwrap();
        let after_delete = list_outline_entries(&project_dir).unwrap();
        assert_eq!(after_delete.len(), 4);
        assert!(
            !after_delete
                .iter()
                .any(|e| e.category == OutlineCategory::Misc)
        );
    }

    #[test]
    fn rename_outline_entry_moves_file_atomically_and_updates_key() {
        let (_root, project_dir) = project_dir();
        create_outline_entry(&project_dir, "旧名", OutlineCategory::Scene, now()).unwrap();

        let renamed =
            rename_outline_entry(&project_dir, OutlineCategory::Scene, "旧名", "新名", now())
                .unwrap();
        assert_eq!(renamed.key, "新名");
        assert_eq!(renamed.category, OutlineCategory::Scene);

        let old_path = outline_entry_path(&project_dir, OutlineCategory::Scene, "旧名").unwrap();
        let new_path = outline_entry_path(&project_dir, OutlineCategory::Scene, "新名").unwrap();
        assert!(!old_path.exists());
        assert!(new_path.is_file());

        let loaded = load_outline_entry(&new_path).unwrap();
        assert_eq!(loaded.key, "新名");
        assert_eq!(loaded.id, renamed.id);
    }

    #[test]
    fn rename_rejects_duplicate_target_key() {
        let (_root, project_dir) = project_dir();
        create_outline_entry(&project_dir, "甲", OutlineCategory::Event, now()).unwrap();
        create_outline_entry(&project_dir, "乙", OutlineCategory::Event, now()).unwrap();
        let err = rename_outline_entry(&project_dir, OutlineCategory::Event, "甲", "乙", now())
            .unwrap_err();
        assert!(
            err.to_string().contains("already exists") || err.to_string().contains("exists"),
            "{err}"
        );
    }

    #[test]
    fn create_rejects_duplicate_key_in_same_category() {
        let (_root, project_dir) = project_dir();
        create_outline_entry(&project_dir, "重复", OutlineCategory::Background, now()).unwrap();
        let err = create_outline_entry(&project_dir, "重复", OutlineCategory::Background, now())
            .unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
    }
}
