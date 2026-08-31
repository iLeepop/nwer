//! Host 跑完后应用到 AppState 的即时 UI 指令（不经提案）。

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiUiCommand {
    OpenChapter { chapter_id: Uuid },
    OpenScript { script_id: Uuid },
    OpenOutline { outline_id: Uuid },
}
