//! 剧本块选中 / 编辑状态机。

/// 剧本块焦点：空闲 → 选中 → 编辑。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ScriptFocus {
    #[default]
    Idle,
    Selected {
        index: usize,
    },
    Editing {
        index: usize,
    },
}

impl ScriptFocus {
    pub fn selected_index(&self) -> Option<usize> {
        match self {
            ScriptFocus::Idle => None,
            ScriptFocus::Selected { index } | ScriptFocus::Editing { index } => Some(*index),
        }
    }

    pub fn is_editing(&self) -> bool {
        matches!(self, ScriptFocus::Editing { .. })
    }

    pub fn is_editing_index(&self, index: usize) -> bool {
        matches!(self, ScriptFocus::Editing { index: i } if *i == index)
    }

    /// 已在编辑同一块时不应重建输入（打断框选 / 角色字段焦点）。
    pub fn should_rebuild_editor_on_press(&self, index: usize) -> bool {
        !self.is_editing_index(index)
    }

    /// 单击块：直接进入编辑。
    pub fn click_block(self, index: usize) -> Self {
        ScriptFocus::Editing { index }
    }

    pub fn click_outside(self) -> Self {
        ScriptFocus::Idle
    }

    pub fn escape(self) -> Self {
        match self {
            ScriptFocus::Editing { index } => ScriptFocus::Selected { index },
            _ => ScriptFocus::Idle,
        }
    }

    pub fn move_selection(self, delta: isize, block_count: usize) -> Self {
        if block_count == 0 {
            return ScriptFocus::Idle;
        }
        let Some(current) = self.selected_index() else {
            let index = if delta < 0 { block_count - 1 } else { 0 };
            return ScriptFocus::Selected { index };
        };
        let next = (current as isize + delta).clamp(0, block_count as isize - 1) as usize;
        ScriptFocus::Selected { index: next }
    }

    pub fn clamp_to_len(self, len: usize) -> Self {
        if len == 0 {
            return ScriptFocus::Idle;
        }
        match self {
            ScriptFocus::Idle => ScriptFocus::Idle,
            ScriptFocus::Selected { index } => ScriptFocus::Selected {
                index: index.min(len - 1),
            },
            ScriptFocus::Editing { index } => ScriptFocus::Editing {
                index: index.min(len - 1),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScriptMultiSelect {
    pub start: usize,
    pub end: usize,
}

impl ScriptMultiSelect {
    pub fn new(a: usize, b: usize) -> Self {
        Self {
            start: a.min(b),
            end: a.max(b),
        }
    }

    pub fn count(&self) -> usize {
        self.end - self.start + 1
    }

    pub fn contains(&self, index: usize) -> bool {
        index >= self.start && index <= self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_enters_editing() {
        assert_eq!(
            ScriptFocus::Idle.click_block(2),
            ScriptFocus::Editing { index: 2 }
        );
    }

    #[test]
    fn escape_clears_focus() {
        assert_eq!(
            ScriptFocus::Editing { index: 1 }.escape(),
            ScriptFocus::Selected { index: 1 }
        );
        assert_eq!(ScriptFocus::Selected { index: 1 }.escape(), ScriptFocus::Idle);
    }

    #[test]
    fn rebuild_editor_skipped_while_editing_same_block() {
        let editing = ScriptFocus::Editing { index: 1 };
        assert!(!editing.should_rebuild_editor_on_press(1));
        assert!(editing.should_rebuild_editor_on_press(0));
    }
}
