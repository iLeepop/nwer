//! 章节与剧本预览文本格式化（无 UI 依赖）。

use crate::models::{Block, BlockType, Chapter, Script as ScriptDoc, ScriptBlock, ScriptBlockType};

/// 章节预览行：每块正文首行缩进两个全角空格。
pub fn chapter_preview_lines(chapter: &Chapter) -> Vec<String> {
    chapter
        .blocks
        .iter()
        .filter_map(chapter_block_preview_line)
        .collect()
}

fn chapter_block_preview_line(block: &Block) -> Option<String> {
    let text = match block.block_type {
        BlockType::SceneBreak if block.content.is_empty() => "—— ✦ ——".to_string(),
        _ if block.content.is_empty() => return None,
        _ => block.content.clone(),
    };
    Some(format!("　　{text}"))
}

/// 剧本预览行：带类型前缀（如「场景描述：」「张三说：」）。
pub fn script_preview_lines(script: &ScriptDoc) -> Vec<String> {
    let mut last_character: Option<String> = None;
    let mut lines = Vec::new();
    for (index, block) in script.blocks.iter().enumerate() {
        if let Some(line) = script_block_preview_line(script, index, block, &mut last_character) {
            lines.push(line);
        }
    }
    lines
}

fn script_block_preview_line(
    script: &ScriptDoc,
    index: usize,
    block: &ScriptBlock,
    last_character: &mut Option<String>,
) -> Option<String> {
    match block.block_type {
        ScriptBlockType::SceneHeading => {
            if block.content.is_empty() {
                return None;
            }
            Some(format!("场景：{}", block.content))
        }
        ScriptBlockType::Action => {
            if block.content.is_empty() {
                return None;
            }
            Some(format!("场景描述：{}", block.content))
        }
        ScriptBlockType::Character => {
            let name = block
                .character
                .as_deref()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    let t = block.content.trim();
                    if t.is_empty() {
                        None
                    } else {
                        Some(t)
                    }
                })?;
            *last_character = Some(name.to_string());
            None
        }
        ScriptBlockType::Dialogue => {
            if block.content.is_empty() {
                return None;
            }
            let name = block
                .character
                .as_deref()
                .filter(|s| !s.is_empty())
                .or(last_character.as_deref())
                .unwrap_or("某人");
            Some(format!("{name}说：{}", block.content))
        }
        ScriptBlockType::Transition => {
            if block.content.is_empty() {
                return None;
            }
            Some(format!("转场：{}", block.content))
        }
        ScriptBlockType::Camera => {
            if block.content.is_empty() {
                return None;
            }
            Some(format!("镜头：{}", block.content))
        }
        ScriptBlockType::Music => {
            if block.content.is_empty() {
                return None;
            }
            let end = script.resolved_span_end_index(index).unwrap_or(index);
            Some(format!(
                "音乐：{} （覆盖：块#{}–#{}）",
                block.content,
                index + 1,
                end + 1
            ))
        }
        ScriptBlockType::Sfx => {
            if block.content.is_empty() {
                return None;
            }
            Some(format!("音效：{}", block.content))
        }
        ScriptBlockType::Mood => {
            if block.content.is_empty() {
                return None;
            }
            let end = script.resolved_span_end_index(index).unwrap_or(index);
            Some(format!(
                "氛围：{} （覆盖：块#{}–#{}）",
                block.content,
                index + 1,
                end + 1
            ))
        }
        ScriptBlockType::Note => {
            if block.content.is_empty() {
                return None;
            }
            Some(format!("（备注：{}）", block.content))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap()
    }

    #[test]
    fn chapter_preview_indents_two_chars() {
        let mut chapter = Chapter::new("测", now());
        chapter.blocks = vec![Block::new(BlockType::Narration, "第一段。", now())];
        let lines = chapter_preview_lines(&chapter);
        assert_eq!(lines, vec!["　　第一段。".to_string()]);
    }

    #[test]
    fn script_preview_formats_dialogue() {
        let mut script = ScriptDoc::new("测", now());
        script.blocks = vec![
            ScriptBlock::new(ScriptBlockType::Action, "张三进门。", now()),
            ScriptBlock::new_character("张三", now()),
            ScriptBlock::new_dialogue("你好。", "张三", now()),
        ];
        let lines = script_preview_lines(&script);
        assert_eq!(
            lines,
            vec![
                "场景描述：张三进门。".to_string(),
                "张三说：你好。".to_string(),
            ]
        );
    }
}
