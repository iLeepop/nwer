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

/// 剧本预览行：带类型前缀；音乐/氛围用「N开始 / N结束」标记区间。
pub fn script_preview_lines(script: &ScriptDoc) -> Vec<String> {
    let mut music_n = 0usize;
    let mut mood_n = 0usize;
    // (start, end, kind, ordinal, content)
    let mut spans: Vec<(usize, usize, &'static str, usize, String)> = Vec::new();
    for (i, block) in script.blocks.iter().enumerate() {
        if block.content.is_empty() || !block.block_type.is_span_cue() {
            continue;
        }
        let end = script.resolved_span_end_index(i).unwrap_or(i);
        let (kind, n) = match block.block_type {
            ScriptBlockType::Music => {
                music_n += 1;
                ("音乐", music_n)
            }
            ScriptBlockType::Mood => {
                mood_n += 1;
                ("氛围", mood_n)
            }
            _ => continue,
        };
        spans.push((i, end, kind, n, block.content.clone()));
    }

    let mut ends_by_index: Vec<Vec<(usize, &'static str, usize)>> =
        vec![Vec::new(); script.blocks.len()];
    for (start, end, kind, n, _) in &spans {
        if *end < ends_by_index.len() {
            ends_by_index[*end].push((*start, *kind, *n));
        }
    }
    // 同一结束位：先开始的后结束（嵌套时外层后关）
    for list in &mut ends_by_index {
        list.sort_by(|a, b| b.0.cmp(&a.0));
    }

    let span_start: std::collections::HashMap<usize, (usize, &'static str, usize, String)> = spans
        .into_iter()
        .map(|(i, _end, kind, n, content)| (i, (i, kind, n, content)))
        .collect();

    let mut last_character: Option<String> = None;
    let mut lines = Vec::new();
    for (index, block) in script.blocks.iter().enumerate() {
        if let Some((_, kind, n, content)) = span_start.get(&index) {
            lines.push(format!("{kind}{n}开始：{content}"));
        } else if let Some(line) =
            script_block_preview_line(block, &mut last_character)
        {
            lines.push(line);
        }
        for (_, kind, n) in &ends_by_index[index] {
            lines.push(format!("{kind}{n}结束"));
        }
    }
    lines
}

fn script_block_preview_line(
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
        ScriptBlockType::Music | ScriptBlockType::Mood => {
            // 由 script_preview_lines 以开始/结束标记输出
            None
        }
        ScriptBlockType::Sfx => {
            if block.content.is_empty() {
                return None;
            }
            Some(format!("音效：{}", block.content))
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

    #[test]
    fn script_preview_span_cues_use_start_end_markers() {
        let mut script = ScriptDoc::new("测", now());
        let music = ScriptBlock::new(ScriptBlockType::Music, "低沉 BGM", now());
        let action = ScriptBlock::new(ScriptBlockType::Action, "雨下个不停。", now());
        let scene = ScriptBlock::new(ScriptBlockType::SceneHeading, "INT. 二", now());
        script.blocks = vec![music, action, scene];
        let lines = script_preview_lines(&script);
        assert_eq!(
            lines,
            vec![
                "音乐1开始：低沉 BGM".to_string(),
                "场景描述：雨下个不停。".to_string(),
                "音乐1结束".to_string(),
                "场景：INT. 二".to_string(),
            ]
        );
    }
}
