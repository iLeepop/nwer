mod atomic_write;
mod chapter_store;
mod chapter_tree;
mod outline_store;
mod path_validation;
mod project_store;

pub use atomic_write::atomic_write;
pub use chapter_store::{load_chapter, save_chapter};
pub use chapter_tree::{
    ChapterNodeKind, ChapterTreeNode, MoveDirection, RelPath, chapters_dir, check_can_move,
    copy_chapter, create_chapter_file, create_directory, delete_node, find_node_by_chapter_id,
    find_node_by_rel, is_nonempty_directory, move_node, move_sibling, rename_node, resolve_rel,
    scan_chapter_tree,
};
pub use outline_store::{load_outline_entry, outline_entry_path, save_outline_entry};
pub use path_validation::validate_storage_name;
pub use project_store::{
    AppConfig, RecentProject, RuleViolation, add_recent_project, check_can_create_chapter,
    check_can_create_directory, config_path, create_project, load_config, load_config_from,
    load_project, remove_recent_project, save_config, save_config_to, save_project,
};
