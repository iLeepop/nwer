use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::Chapter;

use super::atomic_write;

pub fn load_chapter(path: &Path) -> Result<Chapter> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read chapter {}", path.display()))?;
    let chapter: Chapter = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse chapter {}", path.display()))?;
    Ok(chapter)
}

pub fn save_chapter(path: &Path, chapter: &Chapter) -> Result<()> {
    let json = serde_json::to_string_pretty(chapter).context("failed to serialize chapter")?;
    atomic_write(path, format!("{json}\n").as_bytes())
        .with_context(|| format!("failed to write chapter {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    #[test]
    fn chapter_save_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ch-001开篇.json");
        let now = Utc.with_ymd_and_hms(2026, 8, 29, 8, 0, 0).unwrap();
        let chapter = Chapter::new("开篇", now);

        save_chapter(&path, &chapter).unwrap();
        let loaded = load_chapter(&path).unwrap();
        assert_eq!(loaded, chapter);
        assert_eq!(loaded.title, "开篇");
        assert_eq!(loaded.blocks.len(), 1);
    }
}
