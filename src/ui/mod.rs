mod ai_drag;
mod ai_panel;
mod dialog_footer;
pub mod editor;
pub(crate) mod manuscript;
mod selectable_text;
mod settings;
mod sidebar;
mod status_bar;
mod top_bar;
mod workspace;

pub(crate) use ai_drag::AiDragPayload;
pub(crate) use dialog_footer::dialog_ok_cancel_footer;
pub use workspace::Workspace;
