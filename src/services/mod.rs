pub mod autosave;
pub mod word_count;

pub use autosave::{DebounceTimer, SaveAction, SaveTrigger, TEXT_DEBOUNCE_MS, action_for};
pub use word_count::{
    ChapterStats, CharBreakdown, block_counts_toward_words, count_chapter, count_chars,
    update_book_total,
};
