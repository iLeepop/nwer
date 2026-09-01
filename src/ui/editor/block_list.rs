//! 段落块列表：稿面栏宽、类型声部、单击编辑、拖拽排序。

use chrono::Utc;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Sizable as _, button::Button, h_flex, input::Input, input::Textarea, v_flex,
};

use crate::models::{BlockFocus, BlockType};
use crate::ui::manuscript::{
    TextEmphasis, MANUSCRIPT_MEASURE_PX, block_voice,
};
use crate::ui::Workspace;
use crate::ui::editor::block_view::{
    DragBlock, block_container_style, preview_text, render_block_menu, render_drag_handle,
};

actions!(
    editor_blocks,
    [
        SaveDocument,
        EscapeBlockFocus,
        FocusPrevBlock,
        FocusNextBlock,
        DeleteFocusedBlock,
        MergeSelectedBlocks,
        MoveFocusedBlockUp,
        MoveFocusedBlockDown,
        SetTypeNarration,
        SetTypeAside,
        SetTypeDialogue,
        SetTypeThought,
        SetTypeSceneBreak,
        SetTypeNote,
    ]
);

pub fn render_block_list(
    workspace: &Workspace,
    _window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let Some(chapter) = workspace.state.current_chapter.as_ref() else {
        return v_flex()
            .id("block-list-empty")
            .flex_1()
            .w_full()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("选择一个章节以编辑段落块"),
            )
            .into_any_element();
    };

    let blocks = chapter.blocks.clone();
    let focus = workspace.state.block_focus.clone();
    let multi = workspace.state.block_multi_select.clone();

    let mut children: Vec<AnyElement> = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        let voice = block_voice(block.block_type);
        let (border, bg) =
            block_container_style(block.block_type, &focus, index, multi.as_ref(), cx);
        let editing = matches!(focus, BlockFocus::Editing { index: i } if i == index);
        let allows_speaker = block.block_type.allows_speaker();
        let speaker_label = if block.block_type == BlockType::Thought {
            "人物"
        } else {
            "说话人"
        };
        let body_color = match voice.emphasis {
            TextEmphasis::Primary => cx.theme().foreground,
            TextEmphasis::Soft => cx.theme().muted_foreground,
            TextEmphasis::Muted => cx.theme().muted_foreground,
        };

        let body = if editing {
            if let Some((_, input)) = workspace
                .editing_input
                .as_ref()
                .filter(|(i, _)| *i == index)
            {
                v_flex()
                    .gap_1()
                    .pl(px(voice.content_indent_px))
                    .when(voice.center, |this| this.items_center())
                    .when(allows_speaker, |this| {
                        let speaker = workspace
                            .speaker_input
                            .as_ref()
                            .filter(|(i, _)| *i == index)
                            .map(|(_, s)| s.clone());
                        this.child(if let Some(speaker) = speaker {
                            h_flex()
                                .w_full()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(cx.theme().muted_foreground)
                                        .child(speaker_label),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(Input::new(&speaker)),
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
                .pl(px(voice.content_indent_px))
                .when(voice.center, |this| this.items_center())
                .when(allows_speaker, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "{}",
                                block.speaker.as_deref().unwrap_or("（未设置）")
                            )),
                    )
                })
                .child({
                    let mut el = div()
                        .text_sm()
                        .line_height(relative(1.65))
                        .text_color(body_color);
                    if voice.italic {
                        el = el.italic();
                    }
                    if voice.center {
                        el = el.text_center();
                    }
                    el.child(preview_text(block))
                })
                .into_any_element()
        };

        children.push(
            div()
                .group("block-row")
                .id(SharedString::from(format!("block-{index}")))
                .relative()
                .w_full()
                .py_1p5()
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
                            if let Some(anchor) = this.state.block_focus.selected_index() {
                                this.state.set_multi_select(anchor, index);
                            } else {
                                this.state.click_block(index);
                            }
                            this.invalidate_editor_inputs();
                            cx.stop_propagation();
                            cx.notify();
                            return;
                        }
                        // 已在编辑同一块：放行给 Textarea / speaker Input，避免重建打断框选与焦点。
                        if !this.state.block_focus.should_rebuild_editor_on_press(index) {
                            cx.stop_propagation();
                            return;
                        }
                        this.state.click_block(index);
                        this.invalidate_editor_inputs();
                        this.ensure_editing_input(window, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .drag_over::<DragBlock>(|style, _, _, cx| style.bg(cx.theme().drop_target))
                .on_drop(cx.listener(move |this, drag: &DragBlock, window, cx| {
                    if drag.index == index {
                        return;
                    }
                    if let Err(err) = this.state.move_block_at(drag.index, index, Utc::now()) {
                        eprintln!("move block failed: {err:#}");
                    }
                    this.invalidate_editor_inputs();
                    if matches!(this.state.block_focus, BlockFocus::Editing { .. }) {
                        this.ensure_editing_input(window, cx);
                    }
                    cx.notify();
                }))
                .child(render_drag_handle(index, cx))
                .child(
                    div()
                        .absolute()
                        .top_1()
                        .right_1()
                        .invisible()
                        .group_hover("block-row", |this| this.visible())
                        .child(render_block_menu(index, block, cx)),
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
                    Button::new("merge-blocks")
                        .small()
                        .label(format!("合并块 {}–{}", range.start + 1, range.end + 1))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Err(err) = this.state.merge_selected_blocks(Utc::now()) {
                                eprintln!("merge failed: {err:#}");
                            }
                            this.invalidate_editor_inputs();
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
        .id("block-list")
        .flex_1()
        .w_full()
        .items_center()
        .overflow_y_scroll()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.state.click_editor_outside();
                this.invalidate_editor_inputs();
                cx.notify();
            }),
        )
        .child(
            v_flex()
                .id("block-list-measure")
                .w_full()
                .max_w(px(MANUSCRIPT_MEASURE_PX))
                .gap_0p5()
                .px_2()
                .py_1()
                .children(children),
        )
        .into_any_element()
}
