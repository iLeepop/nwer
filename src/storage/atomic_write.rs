use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

/// 同目录临时文件写入，sync 后原子 rename 到目标路径。
/// 第一版不创建 `.bak`。
pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;

    let temp_name = format!(
        ".{}.tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    );
    let temp_path = parent.join(&temp_name);

    let write_result = (|| {
        let mut file = File::create(&temp_path)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    fs::rename(&temp_path, path).inspect_err(|_| {
        let _ = fs::remove_file(&temp_path);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_creates_file_with_contents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("note.json");
        atomic_write(&path, br#"{"ok":true}"#).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), r#"{"ok":true}"#);
    }

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.json");
        fs::write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn atomic_write_leaves_no_temp_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("project.json");
        atomic_write(&path, b"{}").unwrap();
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers.len(), 1);
        assert_eq!(leftovers[0], "project.json");
    }

    #[test]
    fn atomic_write_creates_missing_parent_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("a").join("file.json");
        atomic_write(&path, b"x").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "x");
    }
}
