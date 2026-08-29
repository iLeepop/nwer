//! 自动保存调度（§4.6）：500ms 防抖与立即保存判定。

/// 文本输入默认防抖毫秒。
pub const TEXT_DEBOUNCE_MS: u64 = 500;

/// 保存触发类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveTrigger {
    /// 文本输入：应启动/重置防抖。
    TextEdit,
    /// 增删/排序/类型切换等：立即保存。
    StructuralChange,
    /// 菜单 / Cmd+S / Ctrl+S。
    Manual,
    /// 切换章节、切换项目、退出前。
    BeforeLeave,
}

impl SaveTrigger {
    pub fn is_immediate(self) -> bool {
        !matches!(self, SaveTrigger::TextEdit)
    }
}

/// 纯逻辑防抖计时器（由 UI 传入单调时钟毫秒）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebounceTimer {
    delay_ms: u64,
    /// 计划触发的时刻（单调毫秒）；`None` 表示无待保存防抖。
    due_at_ms: Option<u64>,
}

impl DebounceTimer {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay_ms,
            due_at_ms: None,
        }
    }

    pub fn with_default_delay() -> Self {
        Self::new(TEXT_DEBOUNCE_MS)
    }

    pub fn is_pending(&self) -> bool {
        self.due_at_ms.is_some()
    }

    /// 文本编辑：重置防抖窗口。
    pub fn schedule_from(&mut self, now_ms: u64) {
        self.due_at_ms = Some(now_ms.saturating_add(self.delay_ms));
    }

    pub fn cancel(&mut self) {
        self.due_at_ms = None;
    }

    pub fn is_due(&self, now_ms: u64) -> bool {
        self.due_at_ms.is_some_and(|t| now_ms >= t)
    }

    /// 若到期则清除并返回 true。
    pub fn take_if_due(&mut self, now_ms: u64) -> bool {
        if self.is_due(now_ms) {
            self.due_at_ms = None;
            true
        } else {
            false
        }
    }

    /// 立即保存前取消防抖，避免重复写入。
    pub fn take_pending(&mut self) -> bool {
        self.due_at_ms.take().is_some()
    }
}

/// 根据触发类型决定下一步。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveAction {
    /// 重置防抖，稍后保存。
    ScheduleDebounce,
    /// 立刻落盘。
    SaveNow,
}

pub fn action_for(trigger: SaveTrigger) -> SaveAction {
    if trigger.is_immediate() {
        SaveAction::SaveNow
    } else {
        SaveAction::ScheduleDebounce
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_edit_debounces_500ms() {
        let mut t = DebounceTimer::with_default_delay();
        assert_eq!(
            action_for(SaveTrigger::TextEdit),
            SaveAction::ScheduleDebounce
        );
        t.schedule_from(1_000);
        assert!(t.is_pending());
        assert!(!t.is_due(1_499));
        assert!(t.is_due(1_500));
        assert!(t.take_if_due(1_500));
        assert!(!t.is_pending());
    }

    #[test]
    fn structural_manual_and_leave_are_immediate() {
        for trigger in [
            SaveTrigger::StructuralChange,
            SaveTrigger::Manual,
            SaveTrigger::BeforeLeave,
        ] {
            assert_eq!(action_for(trigger), SaveAction::SaveNow);
        }
    }

    #[test]
    fn reschedule_extends_window() {
        let mut t = DebounceTimer::with_default_delay();
        t.schedule_from(0);
        t.schedule_from(200);
        assert!(!t.is_due(500));
        assert!(t.is_due(700));
    }

    #[test]
    fn take_pending_cancels_debounce() {
        let mut t = DebounceTimer::with_default_delay();
        t.schedule_from(0);
        assert!(t.take_pending());
        assert!(!t.is_pending());
        assert!(!t.take_pending());
    }
}
