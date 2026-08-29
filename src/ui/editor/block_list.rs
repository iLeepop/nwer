//! 段落块列表：选中/编辑、插入、工具条。

use chrono::Utc;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, button::Button, button::ButtonVariants as _,
    h_flex, input::Input, input::Textarea, menu::DropdownMenu as _, menu::PopupMenuItem, v_flex,
};

use crate::models::{BlockFocus, BlockType};
use crate::ui::Workspace;
use crate::ui::editor::block_view::{block_container_style, preview_text, render_block_toolbar};

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
        SetTypeDialogue,
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
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted)
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
    let block_count = blocks.len();

    let mut children: Vec<AnyElement> = Vec::new();

    // 顶部插入点
    children.push(insert_gap(0, cx).into_any_element());

    for (index, block) in blocks.iter().enumerate() {
        let (border, bg) = block_container_style(&focus, index, multi.as_ref(), cx);
        let editing = matches!(focus, BlockFocus::Editing { index: i } if i == index);
        let is_dialogue = block.block_type == BlockType::Dialogue;

        let body = if editing {
            if let Some((_, input)) = workspace
                .editing_input
                .as_ref()
                .filter(|(i, _)| *i == index)
            {
                v_flex()
                    .gap_2()
                    .when(is_dialogue, |this| {
                        let speaker = workspace
                            .speaker_input
                            .as_ref()
                            .filter(|(i, _)| *i == index)
                            .map(|(_, s)| s.clone());
                        this.child(if let Some(speaker) = speaker {
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("说话人"),
                                )
                                .child(Input::new(&speaker))
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        })
                    })
                    .child(Textarea::new(input))
                    .into_any_element()
            } else {
                div()
                    .text_sm()
                    .child(preview_text(block))
                    .into_any_element()
            }
        } else {
            v_flex()
                .gap_1()
                .when(is_dialogue, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!(
                                "说话人：{}",
                                block.speaker.as_deref().unwrap_or("（未设置）")
                            )),
                    )
                })
                .child(div().text_sm().child(preview_text(block)))
                .into_any_element()
        };

        children.push(
            v_flex()
                .id(SharedString::from(format!("block-{index}")))
                .w_full()
                .gap_1()
                .p_2()
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
                            cx.notify();
                            return;
                        }
                        let was_editing = this.state.block_focus.is_editing()
                            && this.state.block_focus.selected_index() == Some(index);
                        this.state.click_block(index);
                        if matches!(this.state.block_focus, BlockFocus::Editing { .. })
                            && !was_editing
                        {
                            this.ensure_editing_input(window, cx);
                        } else if !this.state.block_focus.is_editing() {
                            this.invalidate_editor_inputs();
                        }
                        cx.notify();
                    }),
                )
                .child(render_block_toolbar(index, block, &focus, block_count, cx))
                .child(body)
                .into_any_element(),
        );

        children.push(insert_gap(index + 1, cx).into_any_element());
    }

    if let Some(range) = multi.as_ref()
        && range.count() >= 2
    {
        children.push(
            h_flex()
                .gap_2()
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
        h_flex()
            .gap_2()
            .pt_2()
            .child(
                Button::new("multi-hint")
                    .ghost()
                    .xsmall()
                    .label("多选合并：选中块后 Shift+点击另一块")
                    .disabled(true),
            )
            .into_any_element(),
        // CONCERNS: 拖拽排序以「上移/下移」代替
    );

    v_flex()
        .id("block-list")
        .flex_1()
        .gap_1()
        .p_3()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted)
        .overflow_y_scroll()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                // 点击列表空白处结束编辑（块自身会 stop? 这里作为兜底）
                // 不在此处理，以免抢走块点击；由 Esc / 显式按钮处理
                let _ = this;
                let _ = cx;
            }),
        )
        .children(children)
        .into_any_element()
}

fn insert_gap(insert_index: usize, cx: &mut Context<'_, Workspace>) -> impl IntoElement {
    let workspace = cx.entity();
    h_flex()
        .id(SharedString::from(format!("insert-gap-{insert_index}")))
        .w_full()
        .justify_center()
        .child(
            Button::new(format!("insert-{insert_index}"))
                .ghost()
                .xsmall()
                .label("+")
                .dropdown_menu(move |menu, _, _| {
                    let workspace = workspace.clone();
                    let types = [
                        (BlockType::Narration, "叙述"),
                        (BlockType::Dialogue, "对话"),
                        (BlockType::SceneBreak, "场景分隔"),
                        (BlockType::Note, "备注"),
                    ];
                    let mut menu = menu;
                    for (ty, label) in types {
                        let workspace = workspace.clone();
                        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                            workspace.update(cx, |this, cx| {
                                if let Err(err) =
                                    this.state.insert_block_at(insert_index, ty, Utc::now())
                                {
                                    eprintln!("insert block failed: {err:#}");
                                }
                                this.invalidate_editor_inputs();
                                cx.notify();
                            });
                        }));
                    }
                    menu
                }),
        )
}
