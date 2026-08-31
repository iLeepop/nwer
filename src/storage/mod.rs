mod atomic_write;
mod chapter_store;
mod chapter_tree;
mod outline_store;
mod path_validation;
mod project_store;
mod script_store;
mod script_tree;

pub use atomic_write::atomic_write;
pub use chapter_store::{load_chapter, save_chapter};
pub use chapter_tree::{
    ChapterNodeKind, ChapterTreeNode, MoveDirection, RelPath, chapters_dir, check_can_move,
    copy_chapter, create_chapter_file, create_directory, delete_node, find_node_by_chapter_id,
    find_node_by_rel, is_nonempty_directory, move_node, move_sibling, rename_node, resolve_rel,
    scan_chapter_tree,
};
pub use outline_store::{
    create_outline_entry, delete_outline_entry, list_outline_entries, load_outline_entry,
    outline_entry_path, rename_outline_entry, save_outline_entry,
};
pub use path_validation::validate_storage_name;
pub use project_store::{
    AiSettings, AppConfig, RecentProject, RuleViolation, add_recent_project, ai_providers,
    clamp_max_tool_rounds, default_max_tool_rounds,
    check_can_create_chapter, check_can_create_directory, config_path, create_project,
    default_base_url_for_provider, load_config, load_config_from, load_project,
    remove_recent_project, save_config, save_config_to, save_project, validate_settings_save,
};
pub use script_store::{load_script, save_script};
pub use script_tree::{
    ScriptNodeKind, ScriptTreeNode, copy_script, create_directory as create_script_directory,
    create_script_file, delete_node as delete_script_node,
    find_node_by_rel as find_script_node_by_rel, find_node_by_script_id,
    is_nonempty_directory as script_dir_nonempty, move_node as move_script_node,
    move_sibling as move_script_sibling, rename_node as rename_script_node,
    resolve_rel as resolve_script_rel, scan_script_tree, scripts_dir,
};
