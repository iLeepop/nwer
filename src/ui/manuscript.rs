//! Manuscript Workbench — pure layout decisions for the writing surface.
//!
//! Keeps craft rules out of render glue so they can be tested and reused
//! across chapter blocks, the editor pane, and the status bar.

use crate::models::BlockType;

/// Readable prose column width (px).
pub const MANUSCRIPT_MEASURE_PX: f32 = 720.0;

/// How strongly a block's body should read against the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEmphasis {
    /// Main narrative voice.
    Primary,
    /// Softened (aside / scene ornament).
    Soft,
    /// Demoted (thought / note body).
    Muted,
}

/// Typographic “voice” for a novel paragraph block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockVoice {
    /// Extra left inset on the content column (dialogue / thought).
    pub content_indent_px: f32,
    /// Center the body (scene break).
    pub center: bool,
    pub italic: bool,
    pub emphasis: TextEmphasis,
    /// Idle note blocks keep a quiet card; prose blocks do not.
    pub card_when_idle: bool,
}

/// Status-bar hierarchy for a labeled metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatTier {
    /// Focal number (chapter / script word count).
    Hero,
    /// Supporting total (book words).
    Secondary,
    /// Everything else.
    Meta,
}

/// Whether an idle/selected/editing block should paint card chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSurface {
    pub fill_muted: bool,
    pub emphasize_border: bool,
}

pub fn block_voice(block_type: BlockType) -> BlockVoice {
    match block_type {
        BlockType::Narration => BlockVoice {
            content_indent_px: 0.0,
            center: false,
            italic: false,
            emphasis: TextEmphasis::Primary,
            card_when_idle: false,
        },
        BlockType::Aside => BlockVoice {
            content_indent_px: 8.0,
            center: false,
            italic: true,
            emphasis: TextEmphasis::Soft,
            card_when_idle: false,
        },
        BlockType::Dialogue => BlockVoice {
            content_indent_px: 28.0,
            center: false,
            italic: false,
            emphasis: TextEmphasis::Primary,
            card_when_idle: false,
        },
        BlockType::Thought => BlockVoice {
            content_indent_px: 20.0,
            center: false,
            italic: true,
            emphasis: TextEmphasis::Muted,
            card_when_idle: false,
        },
        BlockType::SceneBreak => BlockVoice {
            content_indent_px: 0.0,
            center: true,
            italic: false,
            emphasis: TextEmphasis::Soft,
            card_when_idle: false,
        },
        BlockType::Note => BlockVoice {
            content_indent_px: 0.0,
            center: false,
            italic: false,
            emphasis: TextEmphasis::Muted,
            card_when_idle: true,
        },
    }
}

pub fn block_surface(voice: BlockVoice, editing: bool, selected: bool) -> BlockSurface {
    let active = editing || selected;
    BlockSurface {
        fill_muted: voice.card_when_idle && !active,
        emphasize_border: editing,
    }
}

pub fn chapter_stat_tier(label: &str) -> StatTier {
    match label {
        "本章总字数" | "剧本字数" => StatTier::Hero,
        "全书总字数" => StatTier::Secondary,
        _ => StatTier::Meta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialogue_and_thought_indent_more_than_narration() {
        let narration = block_voice(BlockType::Narration);
        let dialogue = block_voice(BlockType::Dialogue);
        let thought = block_voice(BlockType::Thought);
        assert!(dialogue.content_indent_px > narration.content_indent_px);
        assert!(thought.content_indent_px > narration.content_indent_px);
        assert!(thought.italic);
        assert_eq!(thought.emphasis, TextEmphasis::Muted);
    }

    #[test]
    fn scene_break_is_centered_ornament() {
        let voice = block_voice(BlockType::SceneBreak);
        assert!(voice.center);
        assert!(!voice.card_when_idle);
        assert_eq!(voice.emphasis, TextEmphasis::Soft);
    }

    #[test]
    fn only_notes_keep_idle_cards() {
        for ty in [
            BlockType::Narration,
            BlockType::Aside,
            BlockType::Dialogue,
            BlockType::Thought,
            BlockType::SceneBreak,
        ] {
            assert!(
                !block_voice(ty).card_when_idle,
                "{ty:?} should not card when idle"
            );
        }
        assert!(block_voice(BlockType::Note).card_when_idle);
    }

    #[test]
    fn surface_emphasizes_border_only_while_editing() {
        let voice = block_voice(BlockType::Narration);
        assert!(!block_surface(voice, false, false).emphasize_border);
        assert!(!block_surface(voice, false, true).emphasize_border);
        assert!(block_surface(voice, true, false).emphasize_border);
    }

    #[test]
    fn note_fills_muted_when_idle_clears_when_active() {
        let voice = block_voice(BlockType::Note);
        assert!(block_surface(voice, false, false).fill_muted);
        assert!(!block_surface(voice, true, false).fill_muted);
        assert!(!block_surface(voice, false, true).fill_muted);
    }

    #[test]
    fn word_count_is_hero_metric() {
        assert_eq!(chapter_stat_tier("本章总字数"), StatTier::Hero);
        assert_eq!(chapter_stat_tier("剧本字数"), StatTier::Hero);
        assert_eq!(chapter_stat_tier("全书总字数"), StatTier::Secondary);
        assert_eq!(chapter_stat_tier("汉字"), StatTier::Meta);
        assert_eq!(chapter_stat_tier("块数"), StatTier::Meta);
    }

    #[test]
    fn manuscript_measure_is_readable_not_full_bleed() {
        assert!(MANUSCRIPT_MEASURE_PX >= 640.0);
        assert!(MANUSCRIPT_MEASURE_PX <= 800.0);
    }
}
