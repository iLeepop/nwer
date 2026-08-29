//! 单个段落块的视觉外壳与操作条。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, button::Button, button::ButtonVariants as _,
    h_flex, menu::DropdownMenu as _, menu::PopupMenu,
};

use crate::models::{Block, BlockFocus, BlockType};
use crate::ui::Workspace;

use super::block_list::{SetTypeDialogue, SetTypeNarration, SetTypeNote, SetTypeSceneBreak};

pub struct BlockChrome;

impl BlockChrome {
    pub fn type_label(t: BlockType) -> &'static str {
        t.label()
    }
}

pub fn render_block_toolbar(
    index: usize,
    block: &Block,
    focus: &BlockFocus,
    block_count: usize,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let selected = focus.selected_index() == Some(index);
    let editing = matches!(focus, BlockFocus::Editing { index: i } if *i == index);

    h_flex()
        .id(SharedString::from(format!("block-toolbar-{index}")))
        .w_full()
        .gap_1()
        .items_center()
        .child(
            div()
                .text_xs()
                .px_1()
                .rounded_sm()
                .bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
                .child(block.block_type.label()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(if editing {
                    "编辑中 · Enter 分割"
                } else if selected {
                    "已选中 · 再点进入编辑"
                } else {
                    "单击选中"
                }),
        )
        .child(div().flex_1())
        .when(selected, |this| {
            this.child(
                Button::new(format!("type-{index}"))
                    .ghost()
                    .xsmall()
                    .label("类型")
                    .dropdown_menu(|menu, _, _| type_menu(menu)),
            )
            // CONCERNS: 拖拽排序以「上移/下移」代替
            .child(
                Button::new(format!("up-{index}"))
                    .ghost()
                    .xsmall()
                    .label("上移")
                    .disabled(index == 0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Err(err) = this.state.swap_block_at(index, true, chrono::Utc::now())
                        {
                            eprintln!("move block up failed: {err:#}");
                        }
                        this.invalidate_editor_inputs();
                        cx.notify();
                    })),
            )
            .child(
                Button::new(format!("down-{index}"))
                    .ghost()
                    .xsmall()
                    .label("下移")
                    .disabled(index + 1 >= block_count)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Err(err) = this.state.swap_block_at(index, false, chrono::Utc::now())
                        {
                            eprintln!("move block down failed: {err:#}");
                        }
                        this.invalidate_editor_inputs();
                        cx.notify();
                    })),
            )
            .child(
                Button::new(format!("del-{index}"))
                    .ghost()
                    .xsmall()
                    .label("删除")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.state.block_focus = crate::models::BlockFocus::Selected { index };
                        this.confirm_delete_block(window, cx);
                    })),
            )
        })
}

fn type_menu(menu: PopupMenu) -> PopupMenu {
    menu.menu("叙述", Box::new(SetTypeNarration))
        .menu("对话", Box::new(SetTypeDialogue))
        .menu("场景分隔", Box::new(SetTypeSceneBreak))
        .menu("备注", Box::new(SetTypeNote))
}

pub fn preview_text(block: &Block) -> String {
    match block.block_type {
        BlockType::SceneBreak => {
            if block.content.is_empty() {
                "—— ✦ ——".to_string()
            } else {
                block.content.clone()
            }
        }
        _ => {
            if block.content.is_empty() {
                "（空）".to_string()
            } else {
                block.content.clone()
            }
        }
    }
}

pub fn block_container_style(
    focus: &BlockFocus,
    index: usize,
    multi: Option<&crate::models::BlockMultiSelect>,
    cx: &App,
) -> (Hsla, Hsla) {
    let in_multi = multi.is_some_and(|m| m.contains(index));
    let selected = focus.selected_index() == Some(index) || in_multi;
    let editing = matches!(focus, BlockFocus::Editing { index: i } if *i == index);
    let border = if editing {
        cx.theme().accent
    } else {
        cx.theme().border
    };
    let bg = if editing || selected {
        cx.theme().background
    } else {
        cx.theme().muted
    };
    (border, bg)
}
