use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::Script;

use super::atomic_write;

pub fn load_script(path: &Path) -> Result<Script> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("failed to read script {}", path.display()))?;
    let script: Script = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse script {}", path.display()))?;
    Ok(script)
}

pub fn save_script(path: &Path, script: &Script) -> Result<()> {
    let json = serde_json::to_string_pretty(script).context("failed to serialize script")?;
    atomic_write(path, format!("{json}\n").as_bytes())
        .with_context(|| format!("failed to write script {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    #[test]
    fn script_save_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("ep-001.json");
        let now = Utc.with_ymd_and_hms(2026, 8, 31, 8, 0, 0).unwrap();
        let script = Script::new("第一集", now);

        save_script(&path, &script).unwrap();
        let loaded = load_script(&path).unwrap();
        assert_eq!(loaded, script);
        assert_eq!(loaded.title, "第一集");
        assert_eq!(loaded.blocks.len(), 1);
    }
}
