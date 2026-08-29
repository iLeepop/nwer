use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

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
        let now = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let mut entry = OutlineEntry::new("张三", OutlineCategory::Character, now);
        entry.fields.insert("身份".into(), "弟子".into());

        save_outline_entry(&path, &entry).unwrap();
        let loaded = load_outline_entry(&path).unwrap();
        assert_eq!(loaded, entry);
    }
}
