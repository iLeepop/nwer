use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, button::Button, button::ButtonVariants as _,
    h_flex, list::ListItem, menu::ContextMenuExt as _, menu::PopupMenu, v_flex,
};
use uuid::Uuid;

use crate::app::AppState;
use crate::models::{OutlineCategory, OutlineEntry};
use crate::ui::Workspace;

actions!(
    outline_tree,
    [NewOutlineEntry, RenameOutlineEntry, DeleteOutlineEntry,]
);

pub fn render_outline_tree(state: &AppState, cx: &mut Context<'_, Workspace>) -> impl IntoElement {
    let has_project = state.project_dir.is_some();
    let entries = state.displayed_outline_entries();
    let selected_id = state.current_outline.as_ref().map(|e| e.id);

    v_flex()
        .id("outline-tree")
        .size_full()
        .gap_1()
        .child(toolbar(has_project, selected_id.is_some(), cx))
        .child(
            v_flex()
                .id("outline-tree-nodes")
                .flex_1()
                .gap_1()
                .overflow_y_scroll()
                .children(if !has_project {
                    vec![empty_hint(cx, "打开或新建项目后显示大纲")]
                } else {
                    render_grouped(&entries, selected_id, state, cx)
                }),
        )
}

fn empty_hint(cx: &App, text: &'static str) -> AnyElement {
    div()
        .p_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

fn toolbar(
    has_project: bool,
    has_selection: bool,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    h_flex()
        .gap_1()
        .flex_wrap()
        .child(
            Button::new("outline-new")
                .ghost()
                .xsmall()
                .label("新建")
                .disabled(!has_project)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.prompt_new_outline(window, cx);
                })),
        )
        .child(
            Button::new("outline-rename")
                .ghost()
                .xsmall()
                .label("重命名")
                .disabled(!has_selection)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.prompt_rename_outline(window, cx);
                })),
        )
        .child(
            Button::new("outline-delete")
                .ghost()
                .xsmall()
                .label("删除")
                .disabled(!has_selection)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.confirm_delete_outline(window, cx);
                })),
        )
}

fn render_grouped(
    entries: &[OutlineEntry],
    selected_id: Option<Uuid>,
    state: &AppState,
    cx: &mut Context<'_, Workspace>,
) -> Vec<AnyElement> {
    let mut rows = Vec::new();
    for category in OutlineCategory::all() {
        let expanded = state.is_outline_category_expanded(category);
        let in_cat: Vec<_> = entries.iter().filter(|e| e.category == category).collect();
        let chevron = if expanded { "▾" } else { "▴" };
        rows.push(
            div()
                .id(SharedString::from(format!("outline-cat-{}", category.label())))
                .px_2()
                .pt_2()
                .pb_1()
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Err(err) = this.state.toggle_outline_category(category) {
                        eprintln!("toggle outline category failed: {err:#}");
                    }
                    cx.notify();
                }))
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(cx.theme().muted_foreground)
                        .child(chevron)
                        .child(category.label()),
                )
                .into_any_element(),
        );
        if !expanded {
            continue;
        }
        if in_cat.is_empty() {
            rows.push(
                div()
                    .px_3()
                    .pb_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("（空）")
                    .into_any_element(),
            );
        } else {
            for entry in in_cat {
                let id = entry.id;
                let key = entry.key.clone();
                let selected = selected_id == Some(id);
                let drag_payload = crate::ui::AiDragPayload {
                    context: crate::ai::AiContextRef {
                        kind: crate::ai::AiContextKind::OutlineEntry,
                        id: Some(id),
                        path: None,
                        title: key.clone(),
                    },
                };
                rows.push(
                    div()
                        .id(SharedString::from(format!("outline-wrap-{id}")))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, _, _, cx| {
                                if let Err(err) = this.state.select_outline(id, Utc::now()) {
                                    eprintln!("select outline failed: {err:#}");
                                }
                                cx.notify();
                            }),
                        )
                        .on_drag(drag_payload, |drag, _, _, cx| cx.new(|_| drag.clone()))
                        .context_menu(move |menu, _, _| build_context_menu(menu))
                        .child(
                            ListItem::new(format!("outline-{id}"))
                                .w_full()
                                .rounded(cx.theme().radius)
                                .selected(selected)
                                .pl(px(16.))
                                .child(div().text_sm().child(key))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Err(err) = this.state.select_outline(id, Utc::now()) {
                                        eprintln!("select outline failed: {err:#}");
                                    }
                                    this.invalidate_editor_inputs();
                                    this.title_input = None;
                                    this.invalidate_outline_inputs();
                                    this.ensure_outline_form(window, cx);
                                    cx.notify();
                                })),
                        )
                        .into_any_element(),
                );
            }
        }
    }
    rows
}

fn build_context_menu(menu: PopupMenu) -> PopupMenu {
    menu.menu("新建条目", Box::new(NewOutlineEntry))
        .menu("重命名", Box::new(RenameOutlineEntry))
        .separator()
        .menu("删除", Box::new(DeleteOutlineEntry))
}
