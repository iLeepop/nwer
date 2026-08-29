use anyhow::{Result, bail};

/// 校验用作目录/文件名片段的字符串（项目标题、大纲 key 等）。
pub fn validate_storage_name(name: &str) -> Result<()> {
    if name.is_empty() || name.trim().is_empty() {
        bail!("storage name must not be empty");
    }
    if name == "." || name == ".." {
        bail!("storage name must not be '.' or '..'");
    }
    if name.contains("..") {
        bail!("storage name must not contain '..'");
    }
    if name.contains('/') || name.contains('\\') {
        bail!("storage name must not contain path separators");
    }
    if name.contains('\0') {
        bail!("storage name must not contain null bytes");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        for name in ["示例小说", "张三", "ch-001开篇", "vol-001"] {
            validate_storage_name(name).unwrap();
        }
    }

    #[test]
    fn rejects_empty_and_whitespace() {
        for name in ["", "   ", "\t"] {
            let err = validate_storage_name(name).unwrap_err();
            assert!(err.to_string().contains("empty"), "{name:?}: {err}");
        }
    }

    #[test]
    fn rejects_dot_segments() {
        for name in [".", "..", "foo/..", "../evil", "..\\evil", "a..b"] {
            let err = validate_storage_name(name).unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("..") || msg.contains("'.'"), "{name:?}: {msg}");
        }
    }

    #[test]
    fn rejects_path_separators() {
        for name in ["foo/bar", "foo\\bar", "/abs", "rel/"] {
            let err = validate_storage_name(name).unwrap_err();
            assert!(
                err.to_string().contains("path separators"),
                "{name:?}: {err}"
            );
        }
    }
}
