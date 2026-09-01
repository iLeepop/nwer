//! 单个剧本块的视觉外壳：拖拽手柄、悬停 ⋮ 菜单、标准剧本排版。

use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _, WindowExt as _, button::Button, button::ButtonVariants as _,
    menu::DropdownMenu as _, menu::PopupMenu, menu::PopupMenuItem,
};

use crate::models::{ScriptBlock, ScriptBlockType, ScriptFocus};
use crate::ui::Workspace;

/// 拖拽排序载荷。
#[derive(Clone)]
pub struct DragScriptBlock {
    pub index: usize,
}

impl Render for DragScriptBlock {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .text_xs()
            .bg(cx.theme().accent)
            .text_color(cx.theme().accent_foreground)
            .rounded_sm()
            .child(format!("块 {}", self.index + 1))
    }
}

pub fn render_script_drag_handle(index: usize, cx: &mut Context<'_, Workspace>) -> impl IntoElement {
    let drag = DragScriptBlock { index };
    div()
        .id(SharedString::from(format!("script-drag-{index}")))
        .absolute()
        .top_2()
        .left_1()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .cursor_grab()
        .child("⠿")
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_drag(drag, |drag, _, _, cx| cx.new(|_| drag.clone()))
}

pub fn render_script_menu(
    index: usize,
    block: &ScriptBlock,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let workspace = cx.entity();
    let current_type = block.block_type;

    Button::new(format!("script-menu-{index}"))
        .ghost()
        .xsmall()
        .icon(IconName::EllipsisVertical)
        .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, window, cx| {
            let workspace = workspace.clone();
            let mut menu = menu.item(PopupMenuItem::label("类型"));
            for ty in ScriptBlockType::all() {
                let workspace = workspace.clone();
                let label = ty.label();
                let mark = if ty == current_type { " ✓" } else { "" };
                menu = menu.item(PopupMenuItem::new(format!("{label}{mark}")).on_click(
                    move |_, _, cx| {
                        workspace.update(cx, |this, cx| {
                            if let Err(err) =
                                this.state.set_script_block_type_at(index, ty, Utc::now())
                            {
                                eprintln!("set script block type failed: {err:#}");
                            }
                            this.invalidate_script_editor_inputs();
                            cx.notify();
                        });
                    },
                ));
            }
            if index == 0 {
                let ws = workspace.clone();
                menu = menu.submenu("在上方插入", window, cx, move |sub, _, cx| {
                    append_insert_items(sub, ws.clone(), 0, cx)
                });
            }
            let ws_below = workspace.clone();
            let insert_below = index + 1;
            menu = menu.submenu("在下方插入", window, cx, move |sub, _, cx| {
                append_insert_items(sub, ws_below.clone(), insert_below, cx)
            });
            let ws_delete = workspace.clone();
            menu.separator()
                .item(PopupMenuItem::new("删除").on_click(move |_, window, cx| {
                    let ws = ws_delete.clone();
                    let idx = index;
                    window.open_alert_dialog(cx, move |alert, _, _| {
                        let ws_ok = ws.clone();
                        alert
                            .title("确认删除剧本块")
                            .description(format!("确定删除第 {} 个剧本块？", idx + 1))
                            .show_cancel(true)
                            .on_ok(move |_, _, cx| {
                                if let Err(err) = ws_ok.update(cx, |this, cx| {
                                    this.state.delete_script_block_at(idx, Utc::now())?;
                                    this.invalidate_script_editor_inputs();
                                    cx.notify();
                                    anyhow::Ok(())
                                }) {
                                    eprintln!("delete script block failed: {err:#}");
                                }
                                true
                            })
                    });
                }))
        })
}

fn append_insert_items(
    mut menu: PopupMenu,
    workspace: Entity<Workspace>,
    insert_index: usize,
    _cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    for ty in ScriptBlockType::all() {
        let workspace = workspace.clone();
        let label = ty.label();
        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
            workspace.update(cx, |this, cx| {
                if let Err(err) = this.state.insert_script_block_at(insert_index, ty, Utc::now())
                {
                    eprintln!("insert script block failed: {err:#}");
                }
                this.invalidate_script_editor_inputs();
                cx.notify();
            });
        }));
    }
    menu
}

pub fn preview_text(block: &ScriptBlock) -> String {
    match block.block_type {
        ScriptBlockType::Dialogue if block.content.is_empty() => {
            "（空对白）".to_string()
        }
        ScriptBlockType::Character if block.content.is_empty() => {
            "（角色名）".to_string()
        }
        ScriptBlockType::Dialogue => block.content.clone(),
        _ if block.content.is_empty() => "（空）".to_string(),
        ScriptBlockType::SceneHeading | ScriptBlockType::Transition | ScriptBlockType::Character => {
            block.content.to_uppercase()
        }
        _ => block.content.clone(),
    }
}

pub fn script_container_style(
    focus: &ScriptFocus,
    index: usize,
    multi: Option<&crate::models::ScriptMultiSelect>,
    cx: &App,
) -> (Hsla, Hsla) {
    let in_multi = multi.is_some_and(|m| m.contains(index));
    let selected = focus.selected_index() == Some(index) || in_multi;
    let editing = matches!(focus, ScriptFocus::Editing { index: i } if *i == index);
    let border = if editing {
        cx.theme().accent
    } else if selected {
        cx.theme().border
    } else {
        cx.theme().border.opacity(0.0)
    };
    let bg = if selected && !editing {
        cx.theme().muted.opacity(0.45)
    } else {
        cx.theme().background
    };
    (border, bg)
}
