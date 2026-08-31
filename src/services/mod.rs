pub mod autosave;
pub mod preview;
pub mod search;
pub mod word_count;

pub use preview::{chapter_preview_lines, script_preview_lines};

pub use autosave::{DebounceTimer, SaveAction, SaveTrigger, TEXT_DEBOUNCE_MS, action_for};
pub use search::{
    FullTextHit, SearchMode, block_participates_in_full_text, filter_chapter_tree_by_name,
    filter_outline_by_name, filter_script_tree_by_name, make_snippet, search_full_text,
};
pub use word_count::{
    ChapterStats, CharBreakdown, ScriptStats, block_counts_toward_words, count_chapter,
    count_chars, count_script, update_book_total,
};
