//! 剧本块列表：紧凑布局、单击编辑、拖拽排序。

use chrono::Utc;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Sizable as _, button::Button, h_flex, input::Input, input::Textarea, v_flex,
};

use crate::models::{ScriptBlockType, ScriptFocus};
use crate::ui::manuscript::MANUSCRIPT_MEASURE_PX;
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
            .w_full()
            .items_center()
            .justify_center()
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
    let picking = workspace.state.script_ends_at_picking;
    let span_hints: Vec<Option<String>> = blocks
        .iter()
        .enumerate()
        .map(|(i, block)| {
            if !block.block_type.is_span_cue() {
                return None;
            }
            if let Some(end_id) = block.ends_at {
                let summary = script
                    .blocks
                    .iter()
                    .find(|b| b.id == end_id)
                    .map(|b| {
                        let t = b.content.trim();
                        if t.is_empty() {
                            format!("至：第 {} 块", script.blocks.iter().position(|x| x.id == end_id).map(|p| p + 1).unwrap_or(0))
                        } else {
                            let short: String = t.chars().take(12).collect();
                            format!("至：{short}")
                        }
                    })
                    .unwrap_or_else(|| "至：（目标已失效，将用默认）".into());
                Some(summary)
            } else {
                script.default_span_end_hint(i).map(|s| s.to_string())
            }
        })
        .collect();

    let mut children: Vec<AnyElement> = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        let (border, bg) = script_container_style(&focus, index, multi.as_ref(), cx);
        let editing = matches!(focus, ScriptFocus::Editing { index: i } if i == index);
        let allows_character = block.block_type.allows_character();
        let is_note = block.block_type == ScriptBlockType::Note;
        let is_picking_source = picking == Some(index);
        let span_hint = span_hints.get(index).and_then(|h| h.clone());
        let cue_label = match block.block_type {
            ScriptBlockType::Music => Some("音乐"),
            ScriptBlockType::Sfx => Some("音效"),
            ScriptBlockType::Mood => Some("氛围"),
            ScriptBlockType::Camera => Some("镜头"),
            _ => None,
        };

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
                                .w_full()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("角色"),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(Input::new(&character)),
                                )
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
                .when(cue_label.is_some(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(cue_label.unwrap_or_default()),
                    )
                })
                .when(span_hint.is_some(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(span_hint.clone().unwrap_or_default()),
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

        let row_border = if is_picking_source {
            cx.theme().accent
        } else {
            border
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
                .border_color(row_border)
                .bg(bg)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        if this.state.script_ends_at_picking.is_some() {
                            match this
                                .state
                                .try_complete_script_ends_at_pick(index, Utc::now())
                            {
                                Ok(true) => {
                                    this.invalidate_script_editor_inputs();
                                    cx.stop_propagation();
                                    cx.notify();
                                    return;
                                }
                                Ok(false) => {}
                                Err(err) => {
                                    eprintln!("set ends_at failed: {err:#}");
                                    this.state.cancel_script_ends_at_pick();
                                    cx.stop_propagation();
                                    cx.notify();
                                    return;
                                }
                            }
                        }
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
                        if !this.state.script_focus.should_rebuild_editor_on_press(index) {
                            cx.stop_propagation();
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
                .child(render_script_drag_handle(
                    index,
                    crate::ai::AiContextRef {
                        kind: crate::ai::AiContextKind::ScriptBlock,
                        id: Some(block.id),
                        path: None,
                        title: {
                            let preview = preview_text(block);
                            let t = preview.chars().take(24).collect::<String>();
                            if preview.chars().count() > 24 {
                                format!("{t}…")
                            } else if t.is_empty() {
                                format!("块 {}", index + 1)
                            } else {
                                t
                            }
                        },
                    },
                    cx,
                ))
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
        .w_full()
        .items_center()
        .overflow_y_scroll()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.state.click_script_editor_outside();
                this.invalidate_script_editor_inputs();
                cx.notify();
            }),
        )
        .child(
            v_flex()
                .id("script-list-measure")
                .w_full()
                .max_w(px(MANUSCRIPT_MEASURE_PX))
                .gap_0p5()
                .px_2()
                .py_1()
                .children(children),
        )
        .into_any_element()
}
