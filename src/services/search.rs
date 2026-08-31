//! 名称过滤与全文搜索。

use std::path::Path;

use anyhow::Result;
use uuid::Uuid;

use crate::models::{BlockType, OutlineEntry};
use crate::storage::{ChapterTreeNode, RelPath, ScriptTreeNode};

/// 搜索模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    /// 匹配目录名、章节标题、大纲条目名；保留祖先路径。
    #[default]
    NameFilter,
    /// 扫描章节可编辑文本块 content。
    FullText,
}

/// 全文搜索命中。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullTextHit {
    pub chapter_rel: RelPath,
    pub chapter_title: String,
    pub chapter_id: Uuid,
    pub block_id: Uuid,
    pub block_index: usize,
    pub block_type: BlockType,
    /// 命中附近摘要（含省略号）。
    pub snippet: String,
}

/// 名称过滤：保留匹配节点及其祖先。空查询返回原树。
pub fn filter_chapter_tree_by_name(nodes: &[ChapterTreeNode], query: &str) -> Vec<ChapterTreeNode> {
    let q = query.trim();
    if q.is_empty() {
        return nodes.to_vec();
    }
    nodes.iter().filter_map(|n| filter_node(n, q)).collect()
}

fn filter_node(node: &ChapterTreeNode, query: &str) -> Option<ChapterTreeNode> {
    let self_match = node_name_matches(node, query);
    let filtered_children: Vec<_> = node
        .children
        .iter()
        .filter_map(|c| filter_node(c, query))
        .collect();

    if self_match || !filtered_children.is_empty() {
        let mut cloned = node.clone();
        // 若自身匹配，保留全部子节点；否则仅保留匹配子树
        if !self_match {
            cloned.children = filtered_children;
        }
        Some(cloned)
    } else {
        None
    }
}

fn node_name_matches(node: &ChapterTreeNode, query: &str) -> bool {
    let q = query.to_lowercase();
    if node.name.to_lowercase().contains(&q) {
        return true;
    }
    if let Some(title) = &node.title
        && title.to_lowercase().contains(&q)
    {
        return true;
    }
    false
}

/// 名称过滤剧本树：规则同章节树。
pub fn filter_script_tree_by_name(nodes: &[ScriptTreeNode], query: &str) -> Vec<ScriptTreeNode> {
    let q = query.trim();
    if q.is_empty() {
        return nodes.to_vec();
    }
    nodes
        .iter()
        .filter_map(|n| filter_script_node(n, q))
        .collect()
}

fn filter_script_node(node: &ScriptTreeNode, query: &str) -> Option<ScriptTreeNode> {
    let self_match = script_node_name_matches(node, query);
    let filtered_children: Vec<_> = node
        .children
        .iter()
        .filter_map(|c| filter_script_node(c, query))
        .collect();

    if self_match || !filtered_children.is_empty() {
        let mut cloned = node.clone();
        if !self_match {
            cloned.children = filtered_children;
        }
        Some(cloned)
    } else {
        None
    }
}

fn script_node_name_matches(node: &ScriptTreeNode, query: &str) -> bool {
    let q = query.to_lowercase();
    if node.name.to_lowercase().contains(&q) {
        return true;
    }
    if let Some(title) = &node.title
        && title.to_lowercase().contains(&q)
    {
        return true;
    }
    false
}

/// 按条目 key 过滤大纲；空查询返回全部。
pub fn filter_outline_by_name(entries: &[OutlineEntry], query: &str) -> Vec<OutlineEntry> {
    let q = query.trim();
    if q.is_empty() {
        return entries.to_vec();
    }
    let q = q.to_lowercase();
    entries
        .iter()
        .filter(|e| e.key.to_lowercase().contains(&q))
        .cloned()
        .collect()
}

/// 全文搜索：扫描章节可编辑块。`note` 参与，`scene_break` 不参与。
pub fn search_full_text(project_dir: &Path, query: &str) -> Result<Vec<FullTextHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    use crate::storage::scan_chapter_tree;

    let tree = scan_chapter_tree(project_dir)?;
    let mut hits = Vec::new();
    collect_full_text(&tree, project_dir, q, &mut hits)?;
    Ok(hits)
}

fn collect_full_text(
    nodes: &[ChapterTreeNode],
    project_dir: &Path,
    query: &str,
    hits: &mut Vec<FullTextHit>,
) -> Result<()> {
    use crate::storage::{load_chapter, resolve_rel};

    for node in nodes {
        if node.is_chapter() {
            let path = resolve_rel(project_dir, &node.rel_path)?;
            let chapter = load_chapter(&path)?;
            let title = chapter.title.clone();
            let chapter_id = chapter.id;
            for (block_index, block) in chapter.blocks.iter().enumerate() {
                if block.block_type == BlockType::SceneBreak {
                    continue;
                }
                if let Some(snippet) = make_snippet(&block.content, query) {
                    hits.push(FullTextHit {
                        chapter_rel: node.rel_path.clone(),
                        chapter_title: title.clone(),
                        chapter_id,
                        block_id: block.id,
                        block_index,
                        block_type: block.block_type,
                        snippet,
                    });
                }
            }
        } else {
            collect_full_text(&node.children, project_dir, query, hits)?;
        }
    }
    Ok(())
}

/// 生成命中摘要：命中前后各约 12 字，超出加省略号。
pub fn make_snippet(content: &str, query: &str) -> Option<String> {
    let lower = content.to_lowercase();
    let q = query.to_lowercase();
    let byte_pos = lower.find(&q)?;
    let chars: Vec<char> = content.chars().collect();
    // 将 byte 偏移转为 char 索引
    let prefix_bytes = &content[..byte_pos];
    let start_char = prefix_bytes.chars().count();
    let query_len = query.chars().count();
    let end_char = (start_char + query_len).min(chars.len());

    let ctx = 12usize;
    let from = start_char.saturating_sub(ctx);
    let to = (end_char + ctx).min(chars.len());
    let mut snippet: String = chars[from..to].iter().collect();
    if from > 0 {
        snippet = format!("…{snippet}");
    }
    if to < chars.len() {
        snippet.push('…');
    }
    Some(snippet)
}

/// 块是否参与全文搜索。
pub fn block_participates_in_full_text(block_type: BlockType) -> bool {
    block_type != BlockType::SceneBreak
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::models::{Block, Chapter, OutlineCategory, OutlineEntry};
    use crate::storage::{
        create_chapter_file, create_directory, create_outline_entry, create_project,
        scan_chapter_tree,
    };

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 30, 12, 0, 0).unwrap()
    }

    #[test]
    fn name_filter_keeps_ancestor_path_for_matching_chapter() {
        let root = tempdir().unwrap();
        let (project_dir, _) = create_project(root.path(), "过滤", now()).unwrap();
        let vol = create_directory(&project_dir, "", "vol-001第一卷", 3).unwrap();
        let part = create_directory(&project_dir, &vol, "part-001上篇", 3).unwrap();
        let chapter = Chapter::new("开篇相遇", now());
        create_chapter_file(&project_dir, &part, "ch-001开篇", &chapter, 3).unwrap();
        create_chapter_file(
            &project_dir,
            &part,
            "ch-002无关",
            &Chapter::new("无关章节", now()),
            3,
        )
        .unwrap();

        let tree = scan_chapter_tree(&project_dir).unwrap();
        let filtered = filter_chapter_tree_by_name(&tree, "相遇");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "vol-001第一卷");
        assert_eq!(filtered[0].children.len(), 1);
        assert_eq!(filtered[0].children[0].name, "part-001上篇");
        assert_eq!(filtered[0].children[0].children.len(), 1);
        assert_eq!(
            filtered[0].children[0].children[0].title.as_deref(),
            Some("开篇相遇")
        );

        // 目录名匹配时保留该目录下全部子节点
        let by_vol = filter_chapter_tree_by_name(&tree, "第一卷");
        assert_eq!(by_vol[0].children[0].children.len(), 2);
    }

    #[test]
    fn name_filter_matches_outline_entry_keys() {
        let entries = vec![
            OutlineEntry::new("张三", OutlineCategory::Character, now()),
            OutlineEntry::new("李四", OutlineCategory::Character, now()),
            OutlineEntry::new("青云门", OutlineCategory::Background, now()),
        ];
        let filtered = filter_outline_by_name(&entries, "张");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key, "张三");
    }

    #[test]
    fn full_text_hits_include_snippet_and_block_location() {
        let root = tempdir().unwrap();
        let (project_dir, _) = create_project(root.path(), "全文", now()).unwrap();
        let mut chapter = Chapter::new("烛火之夜", now());
        chapter.blocks.clear();
        chapter.blocks.push(Block::new(
            BlockType::Narration,
            "他推开门，屋里的烛火轻轻摇曳。远处传来脚步声。",
            now(),
        ));
        chapter
            .blocks
            .push(Block::new(BlockType::Dialogue, "谁在那里？", now()));
        chapter
            .blocks
            .push(Block::new(BlockType::Note, "备注：烛火意象", now()));
        chapter
            .blocks
            .push(Block::new(BlockType::SceneBreak, "---烛火---", now()));
        let rel = create_chapter_file(&project_dir, "", "ch-001", &chapter, 3).unwrap();

        let hits = search_full_text(&project_dir, "烛火").unwrap();
        // narration + note，不含 scene_break
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|h| h.chapter_rel == rel));
        assert!(hits.iter().all(|h| h.chapter_title == "烛火之夜"));

        let narration = hits
            .iter()
            .find(|h| h.block_type == BlockType::Narration)
            .expect("narration hit");
        assert_eq!(narration.block_index, 0);
        assert!(narration.snippet.contains("烛火"));
        assert_eq!(narration.block_id, chapter.blocks[0].id);

        let note = hits
            .iter()
            .find(|h| h.block_type == BlockType::Note)
            .expect("note hit");
        assert_eq!(note.block_index, 2);

        assert!(!hits.iter().any(|h| h.block_type == BlockType::SceneBreak));
    }

    #[test]
    fn full_text_empty_query_returns_no_hits() {
        let root = tempdir().unwrap();
        let (project_dir, _) = create_project(root.path(), "空查询", now()).unwrap();
        create_outline_entry(&project_dir, "甲", OutlineCategory::Character, now()).unwrap();
        assert!(search_full_text(&project_dir, "  ").unwrap().is_empty());
    }

    #[test]
    fn name_filter_matches_script_tree() {
        use crate::storage::{ScriptNodeKind, ScriptTreeNode};
        let tree = vec![ScriptTreeNode {
            rel_path: "ep01.json".into(),
            name: "ep01".into(),
            kind: ScriptNodeKind::Script,
            script_id: None,
            title: Some("第一集".into()),
            children: vec![],
        }];
        let filtered = filter_script_tree_by_name(&tree, "第一集");
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn make_snippet_adds_ellipsis_around_match() {
        let content = "前缀文字一二三四五六七八九十甲乙丙丁戊己庚辛壬癸后缀文字继续延伸";
        let snippet = make_snippet(content, "甲乙").unwrap();
        assert!(snippet.contains("甲乙"));
        assert!(snippet.starts_with('…'));
        assert!(snippet.ends_with('…'));
    }
}
