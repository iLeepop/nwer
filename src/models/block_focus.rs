//! 段落块选中 / 编辑状态机（§4.1）。

/// 块焦点：空闲 → 选中 → 编辑。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BlockFocus {
    #[default]
    Idle,
    Selected {
        index: usize,
    },
    Editing {
        index: usize,
    },
}

impl BlockFocus {
    pub fn selected_index(&self) -> Option<usize> {
        match self {
            BlockFocus::Idle => None,
            BlockFocus::Selected { index } | BlockFocus::Editing { index } => Some(*index),
        }
    }

    pub fn is_editing(&self) -> bool {
        matches!(self, BlockFocus::Editing { .. })
    }

    /// 第一次单击：选中；再次单击已选中块：进入编辑。
    pub fn click_block(self, index: usize) -> Self {
        match self {
            BlockFocus::Selected { index: i } if i == index => BlockFocus::Editing { index },
            BlockFocus::Editing { index: i } if i == index => BlockFocus::Editing { index },
            _ => BlockFocus::Selected { index },
        }
    }

    /// 单击块外：结束编辑（回到空闲）。
    pub fn click_outside(self) -> Self {
        BlockFocus::Idle
    }

    /// Esc：结束编辑；若已在选中则清空。
    pub fn escape(self) -> Self {
        match self {
            BlockFocus::Editing { index } => BlockFocus::Selected { index },
            _ => BlockFocus::Idle,
        }
    }

    /// Ctrl+↑ / Ctrl+↓：在块间切换选中（编辑中则退出编辑并选中目标）。
    pub fn move_selection(self, delta: isize, block_count: usize) -> Self {
        if block_count == 0 {
            return BlockFocus::Idle;
        }
        let Some(current) = self.selected_index() else {
            let index = if delta < 0 { block_count - 1 } else { 0 };
            return BlockFocus::Selected { index };
        };
        let next = (current as isize + delta).clamp(0, block_count as isize - 1) as usize;
        BlockFocus::Selected { index: next }
    }

    /// 块数量变化后校正索引。
    pub fn clamp_to_len(self, len: usize) -> Self {
        if len == 0 {
            return BlockFocus::Idle;
        }
        match self {
            BlockFocus::Idle => BlockFocus::Idle,
            BlockFocus::Selected { index } => BlockFocus::Selected {
                index: index.min(len - 1),
            },
            BlockFocus::Editing { index } => BlockFocus::Editing {
                index: index.min(len - 1),
            },
        }
    }
}

/// 多选相邻块（用于合并）。存 inclusive 起止；`None` 表示仅用单选焦点。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockMultiSelect {
    pub start: usize,
    pub end: usize,
}

impl BlockMultiSelect {
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
    fn click_selects_then_edits() {
        let mut focus = BlockFocus::Idle;
        focus = focus.click_block(2);
        assert_eq!(focus, BlockFocus::Selected { index: 2 });
        focus = focus.click_block(2);
        assert_eq!(focus, BlockFocus::Editing { index: 2 });
        focus = focus.click_block(2);
        assert_eq!(focus, BlockFocus::Editing { index: 2 });
    }

    #[test]
    fn click_other_block_selects() {
        let focus = BlockFocus::Editing { index: 1 }.click_block(3);
        assert_eq!(focus, BlockFocus::Selected { index: 3 });
    }

    #[test]
    fn escape_and_outside_end_edit() {
        assert_eq!(
            BlockFocus::Editing { index: 1 }.escape(),
            BlockFocus::Selected { index: 1 }
        );
        assert_eq!(BlockFocus::Selected { index: 1 }.escape(), BlockFocus::Idle);
        assert_eq!(
            BlockFocus::Editing { index: 1 }.click_outside(),
            BlockFocus::Idle
        );
    }

    #[test]
    fn move_selection_clamps() {
        let focus = BlockFocus::Selected { index: 0 }.move_selection(-1, 3);
        assert_eq!(focus, BlockFocus::Selected { index: 0 });
        let focus = BlockFocus::Selected { index: 2 }.move_selection(1, 3);
        assert_eq!(focus, BlockFocus::Selected { index: 2 });
        let focus = BlockFocus::Editing { index: 1 }.move_selection(1, 3);
        assert_eq!(focus, BlockFocus::Selected { index: 2 });
    }
}
