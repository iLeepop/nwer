//! 拖入 AI 面板的上下文引用（芯片）。

use uuid::Uuid;

/// 可附加到 AI 会话的引用种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiContextKind {
    ChapterDir,
    Chapter,
    Block,
    OutlineEntry,
    ScriptDir,
    Script,
    ScriptBlock,
}

impl AiContextKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ChapterDir => "章节目录",
            Self::Chapter => "章节",
            Self::Block => "文本块",
            Self::OutlineEntry => "大纲",
            Self::ScriptDir => "剧本目录",
            Self::Script => "剧本",
            Self::ScriptBlock => "剧本块",
        }
    }
}

/// 一条瘦引用：仅元数据，不含正文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiContextRef {
    pub kind: AiContextKind,
    pub id: Option<Uuid>,
    pub path: Option<String>,
    pub title: String,
}

impl AiContextRef {
    pub fn same_target(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.id == other.id
            && self.path.as_deref().unwrap_or("") == other.path.as_deref().unwrap_or("")
    }

    /// 芯片与 lean 文本用的单行描述。
    pub fn display_line(&self) -> String {
        let mut parts = vec![format!("[{}]", self.kind.label()), self.title.clone()];
        if let Some(id) = self.id {
            parts.push(format!("({id})"));
        } else if let Some(path) = self.path.as_ref() {
            parts.push(format!("({path})"));
        }
        parts.join(" ")
    }
}

/// 若尚未存在同目标引用则追加；返回是否新增。
pub fn push_unique(refs: &mut Vec<AiContextRef>, item: AiContextRef) -> bool {
    if refs.iter().any(|r| r.same_target(&item)) {
        return false;
    }
    refs.push(item);
    true
}

/// 格式化「用户附加引用」段；空则返回 None。
pub fn format_attached_refs(refs: &[AiContextRef]) -> Option<String> {
    if refs.is_empty() {
        return None;
    }
    let mut lines = vec!["用户附加引用：".to_string()];
    for r in refs {
        lines.push(format!("- {}", r.display_line()));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ref_chapter(title: &str, id: Uuid) -> AiContextRef {
        AiContextRef {
            kind: AiContextKind::Chapter,
            id: Some(id),
            path: Some("a.json".into()),
            title: title.into(),
        }
    }

    #[test]
    fn push_unique_dedupes_same_kind_id_path() {
        let id = Uuid::nil();
        let mut refs = Vec::new();
        assert!(push_unique(&mut refs, ref_chapter("一", id)));
        assert!(!push_unique(&mut refs, ref_chapter("一改名", id)));
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn format_attached_refs_lists_metadata_only() {
        let id = Uuid::nil();
        let text = format_attached_refs(&[ref_chapter("第三章", id)]).unwrap();
        assert!(text.contains("用户附加引用"));
        assert!(text.contains("章节"));
        assert!(text.contains("第三章"));
        assert!(!text.contains("正文内容"));
    }

    #[test]
    fn format_empty_is_none() {
        assert!(format_attached_refs(&[]).is_none());
    }
}
