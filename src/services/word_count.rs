//! 字数统计（§4.5）。

use unicode_script::{Script, UnicodeScript};

use crate::models::{Block, BlockType, Chapter};

/// 三类字符计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharBreakdown {
    pub han: u64,
    pub punct_space: u64,
    pub other: u64,
}

impl CharBreakdown {
    pub fn total(self) -> u64 {
        self.han + self.punct_space + self.other
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self {
            han: self.han.saturating_add(other.han),
            punct_space: self.punct_space.saturating_add(other.punct_space),
            other: self.other.saturating_add(other.other),
        }
    }
}

/// 本章统计快照（底栏）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChapterStats {
    pub chars: CharBreakdown,
    pub block_count: u64,
    pub dialogue_count: u64,
}

impl ChapterStats {
    pub fn total_words(self) -> u64 {
        self.chars.total()
    }
}

/// 对单个字符串按 Unicode 规则分类计数。
pub fn count_chars(text: &str) -> CharBreakdown {
    let mut out = CharBreakdown::default();
    for c in text.chars() {
        if c.script() == Script::Han {
            out.han += 1;
        } else if is_punct_or_space(c) {
            out.punct_space += 1;
        } else {
            out.other += 1;
        }
    }
    out
}

fn is_punct_or_space(c: char) -> bool {
    if c.is_whitespace() || c.is_ascii_punctuation() {
        return true;
    }
    // 常见 Unicode 标点块（无 general-category 依赖时的实用近似）
    matches!(
        c,
        '\u{00A0}'
            | '\u{00B7}'
            | '\u{2010}'..='\u{2027}'
            | '\u{2030}'..='\u{205E}'
            | '\u{3000}'..='\u{303F}'
            | '\u{30FB}'
            | '\u{FF01}'..='\u{FF0F}'
            | '\u{FF1A}'..='\u{FF20}'
            | '\u{FF3B}'..='\u{FF40}'
            | '\u{FF5B}'..='\u{FF65}'
    )
}

/// 块是否参与字数统计：默认仅 narration/dialogue，并尊重排除列表。
pub fn block_counts_toward_words(block: &Block, exclude: &[BlockType]) -> bool {
    if exclude.contains(&block.block_type) {
        return false;
    }
    block.block_type.counts_toward_word_total()
}

/// 统计章节；`exclude` 来自 `project.settings.word_count_exclude_types`。
pub fn count_chapter(chapter: &Chapter, exclude: &[BlockType]) -> ChapterStats {
    let mut chars = CharBreakdown::default();
    let mut dialogue_count = 0u64;
    for block in &chapter.blocks {
        if block.block_type == BlockType::Dialogue {
            dialogue_count += 1;
        }
        if block_counts_toward_words(block, exclude) {
            chars = chars.saturating_add(count_chars(&block.content));
        }
    }
    ChapterStats {
        chars,
        block_count: chapter.blocks.len() as u64,
        dialogue_count,
    }
}

/// 全书增量：`既有全书 - 章保存前 + 章新总数`。
pub fn update_book_total(book_total: u64, chapter_before: u64, chapter_after: u64) -> u64 {
    book_total
        .saturating_sub(chapter_before)
        .saturating_add(chapter_after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Block;
    use chrono::{TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 30, 1, 0, 0).unwrap()
    }

    #[test]
    fn classifies_han_punct_space_and_other() {
        let s = "汉字， ABC\n";
        let c = count_chars(s);
        assert_eq!(c.han, 2);
        // ， space \n = 3 punct/space；也可含全角
        assert_eq!(c.punct_space, 3);
        assert_eq!(c.other, 3); // A B C
        assert_eq!(c.total(), 8);
    }

    #[test]
    fn excludes_note_scene_break_and_speaker() {
        let mut chapter = Chapter::new("测", now());
        chapter.blocks = vec![
            Block::new(BlockType::Narration, "一二", now()),
            Block::new_dialogue("三。", "说话人甲", now()),
            Block::new(BlockType::Note, "备注汉字很多", now()),
            Block::new(BlockType::SceneBreak, "***", now()),
        ];
        let exclude = [BlockType::Note, BlockType::SceneBreak];
        let stats = count_chapter(&chapter, &exclude);
        // 一二 + 三。 = 3 han + 1 punct
        assert_eq!(stats.chars.han, 3);
        assert_eq!(stats.chars.punct_space, 1);
        assert_eq!(stats.chars.other, 0);
        assert_eq!(stats.total_words(), 4);
        assert_eq!(stats.block_count, 4);
        assert_eq!(stats.dialogue_count, 1);
    }

    #[test]
    fn respect_custom_exclude_types() {
        let mut chapter = Chapter::new("测", now());
        chapter.blocks = vec![
            Block::new(BlockType::Narration, "甲", now()),
            Block::new_dialogue("乙", "谁", now()),
        ];
        // 额外排除 dialogue
        let exclude = [BlockType::Note, BlockType::SceneBreak, BlockType::Dialogue];
        let stats = count_chapter(&chapter, &exclude);
        assert_eq!(stats.chars.han, 1);
        assert_eq!(stats.dialogue_count, 1);
        assert_eq!(stats.total_words(), 1);
    }

    #[test]
    fn book_total_incremental_update() {
        assert_eq!(update_book_total(100, 40, 55), 115);
        assert_eq!(update_book_total(10, 20, 5), 5); // saturating
    }
}
