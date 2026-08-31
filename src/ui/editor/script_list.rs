//! 剧本块列表：紧凑布局、单击编辑、拖拽排序。

use chrono::Utc;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Sizable as _, button::Button, h_flex, input::Input, input::Textarea, v_flex,
};

use crate::models::{ScriptBlockType, ScriptFocus};
use crate::ui::Workspace;
use crate::ui::editor::script_view::{
    DragScriptBlock, preview_text, render_script_drag_handle, render_script_menu,
    script_container_style,
};

pub fn render_script_list(
    workspace: &Workspace,
    _window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let Some(script) = workspace.state.current_script.as_ref() else {
        return v_flex()
            .id("script-list-empty")
            .flex_1()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted)
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("选择一个剧本以编辑剧本块"),
            )
            .into_any_element();
    };

    let blocks = script.blocks.clone();
    let focus = workspace.state.script_focus.clone();
    let multi = workspace.state.script_multi_select.clone();

    let mut children: Vec<AnyElement> = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        let (border, bg) = script_container_style(&focus, index, multi.as_ref(), cx);
        let editing = matches!(focus, ScriptFocus::Editing { index: i } if i == index);
        let allows_character = block.block_type.allows_character();
        let is_note = block.block_type == ScriptBlockType::Note;

        let body = if editing {
            if let Some((_, input)) = workspace
                .editing_input
                .as_ref()
                .filter(|(i, _)| *i == index)
            {
                v_flex()
                    .gap_1()
                    .when(allows_character, |this| {
                        let character = workspace
                            .character_input
                            .as_ref()
                            .filter(|(i, _)| *i == index)
                            .map(|(_, s)| s.clone());
                        this.child(if let Some(character) = character {
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("角色"),
                                )
                                .child(Input::new(&character))
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        })
                    })
                    .child(Textarea::new(input).appearance(false).bordered(false))
                    .into_any_element()
            } else {
                div()
                    .text_sm()
                    .child(preview_text(block))
                    .into_any_element()
            }
        } else {
            v_flex()
                .gap_0p5()
                .when(
                    block.block_type == ScriptBlockType::Dialogue && !allows_character,
                    |this| this,
                )
                .when(allows_character && block.block_type == ScriptBlockType::Dialogue, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "角色：{}",
                                block
                                    .character
                                    .as_deref()
                                    .unwrap_or("（未指定角色）")
                            )),
                    )
                })
                .child({
                    let mut el = div()
                        .text_sm()
                        .line_height(relative(1.6));
                    if is_note {
                        el = el.bg(cx.theme().muted).rounded_sm().p_1().text_xs();
                    }
                    match block.block_type {
                        ScriptBlockType::SceneHeading => {
                            el = el.font_weight(FontWeight::BOLD).py_1();
                        }
                        ScriptBlockType::Character => {
                            el = el.font_weight(FontWeight::BOLD).text_center();
                        }
                        ScriptBlockType::Dialogue => {
                            el = el.px_24();
                        }
                        ScriptBlockType::Transition => {
                            el = el.text_right();
                        }
                        ScriptBlockType::Mood => {
                            el = el.italic();
                        }
                        _ => {}
                    }
                    el.child(preview_text(block))
                })
                .into_any_element()
        };

        children.push(
            div()
                .group("script-row")
                .id(SharedString::from(format!("script-{index}")))
                .relative()
                .w_full()
                .py_1()
                .pl_7()
                .pr_8()
                .rounded_md()
                .border_1()
                .border_color(border)
                .bg(bg)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        if event.modifiers.shift {
                            if let Some(anchor) = this.state.script_focus.selected_index() {
                                this.state.set_script_multi_select(anchor, index);
                            } else {
                                this.state.click_script_block(index);
                            }
                            this.invalidate_script_editor_inputs();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        this.state.click_script_block(index);
                        this.invalidate_script_editor_inputs();
                        this.ensure_script_editing_input(window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .drag_over::<DragScriptBlock>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                .on_drop(cx.listener(move |this, drag: &DragScriptBlock, window, cx| {
                    if drag.index == index {
                        return;
                    }
                    if let Err(err) =
                        this.state.move_script_block_at(drag.index, index, Utc::now())
                    {
                        eprintln!("move script block failed: {err:#}");
                    }
                    this.invalidate_script_editor_inputs();
                    if matches!(this.state.script_focus, ScriptFocus::Editing { .. }) {
                        this.ensure_script_editing_input(window, cx);
                    }
                    cx.notify();
                }))
                .child(render_script_drag_handle(index, cx))
                .child(
                    div()
                        .absolute()
                        .top_1()
                        .right_1()
                        .invisible()
                        .group_hover("script-row", |this| this.visible())
                        .child(render_script_menu(index, block, cx)),
                )
                .child(body)
                .into_any_element(),
        );
    }

    if let Some(range) = multi.as_ref()
        && range.count() >= 2
    {
        children.push(
            h_flex()
                .gap_2()
                .pt_1()
                .child(
                    Button::new("merge-script-blocks")
                        .small()
                        .label(format!("合并块 {}–{}", range.start + 1, range.end + 1))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Err(err) = this.state.merge_selected_script_blocks(Utc::now()) {
                                eprintln!("merge failed: {err:#}");
                            }
                            this.invalidate_script_editor_inputs();
                            cx.notify();
                        })),
                )
                .into_any_element(),
        );
    }

    children.push(
        div()
            .pt_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("Shift+点击多选 · 拖拽 ⠿ 排序 · Enter 分割 · Shift+Enter 块内换行")
            .into_any_element(),
    );

    v_flex()
        .id("script-list")
        .flex_1()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted)
        .overflow_y_scroll()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.state.click_script_editor_outside();
                this.invalidate_script_editor_inputs();
                cx.notify();
            }),
        )
        .children(children)
        .into_any_element()
}
