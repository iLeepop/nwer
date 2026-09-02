use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, button::Button,
    button::ButtonVariants as _, h_flex, list::ListItem, menu::ContextMenuExt as _,
    menu::PopupMenu, v_flex,
};

use crate::app::AppState;
use crate::storage::{ScriptNodeKind, ScriptTreeNode, MoveDirection};
use crate::ui::Workspace;
use crate::ui::sidebar::chapter_tree::{DeleteNode, MoveDown, MoveUp, NewDirectory, RenameNode};

actions!(script_tree, [NewScript, CopyScript]);

pub fn render_script_tree(state: &AppState, cx: &mut Context<'_, Workspace>) -> impl IntoElement {
    let has_project = state.project_dir.is_some();
    let has_selection = state.ui.selected_node.is_some();

    v_flex()
        .id("script-tree")
        .size_full()
        .gap_1()
        .child(toolbar(has_project, has_selection, cx))
        .child(
            v_flex()
                .id("script-tree-nodes")
                .flex_1()
                .gap_0()
                .overflow_y_scroll()
                .children(if !has_project {
                    vec![
                        div()
                            .p_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("打开或新建项目后显示剧本树")
                            .into_any_element(),
                    ]
                } else if state.script_tree.is_empty() {
                    vec![
                        div()
                            .p_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("空目录 — 右键或使用上方按钮新建")
                            .into_any_element(),
                    ]
                } else {
                    state
                        .displayed_script_tree()
                        .iter()
                        .flat_map(|n| render_node(n, 0, state, cx))
                        .collect()
                }),
        )
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
            Button::new("tree-new-dir")
                .ghost()
                .xsmall()
                .label("新目录")
                .disabled(!has_project)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.prompt_new_directory(window, cx);
                })),
        )
        .child(
            Button::new("tree-new-ch")
                .ghost()
                .xsmall()
                .label("新剧本")
                .disabled(!has_project)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.prompt_new_script(window, cx);
                })),
        )
        .child(
            Button::new("tree-up")
                .ghost()
                .xsmall()
                .label("上移")
                .disabled(!has_selection)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(err) = this
                        .state
                        .move_selected_sibling(MoveDirection::Up, Utc::now())
                    {
                        eprintln!("move up failed: {err:#}");
                    }
                    cx.notify();
                })),
        )
        .child(
            Button::new("tree-down")
                .ghost()
                .xsmall()
                .label("下移")
                .disabled(!has_selection)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(err) = this
                        .state
                        .move_selected_sibling(MoveDirection::Down, Utc::now())
                    {
                        eprintln!("move down failed: {err:#}");
                    }
                    cx.notify();
                })),
        )
}

fn render_node(
    node: &ScriptTreeNode,
    depth: u32,
    state: &AppState,
    cx: &mut Context<'_, Workspace>,
) -> Vec<AnyElement> {
    let rel = node.rel_path.clone();
    let kind = node.kind;
    let label = match kind {
        ScriptNodeKind::Directory => node.name.clone(),
        ScriptNodeKind::Script => node.title.clone().unwrap_or_else(|| node.name.clone()),
    };
    let expanded = state.is_expanded(&rel);
    let selected = state.ui.selected_node.as_deref() == Some(rel.as_str());
    let is_dir = kind == ScriptNodeKind::Directory;

    let icon = if is_dir {
        if expanded {
            IconName::FolderOpen
        } else {
            IconName::Folder
        }
    } else {
        IconName::File
    };

    let mut rows = Vec::new();
    let drag_payload = crate::ui::AiDragPayload {
        context: crate::ai::AiContextRef {
            kind: if is_dir {
                crate::ai::AiContextKind::ScriptDir
            } else {
                crate::ai::AiContextKind::Script
            },
            id: node.script_id,
            path: Some(rel.clone()),
            title: label.clone(),
        },
    };
    rows.push(
        div()
            .id(SharedString::from(format!("wrap-{rel}")))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let rel = rel.clone();
                    move |this, _, _, cx| {
                        if is_dir {
                            this.state.select_directory(&rel);
                        } else if let Err(err) = this.state.select_script(&rel, Utc::now()) {
                            eprintln!("select chapter failed: {err:#}");
                        }
                        cx.notify();
                    }
                }),
            )
            .on_drag(drag_payload, |drag, _, _, cx| cx.new(|_| drag.clone()))
            .context_menu(move |menu, _window, _cx| build_context_menu(menu, is_dir))
            .child(
                ListItem::new(format!("node-{rel}"))
                    .w_full()
                    .rounded(cx.theme().radius)
                    .selected(selected)
                    .pl(px(12.) + px(14.) * depth as f32)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(icon)
                            .child(div().text_sm().child(label)),
                    )
                    .on_click(cx.listener({
                        let rel = rel.clone();
                        move |this, _, window, cx| {
                            if is_dir {
                                let _ = this.state.toggle_expanded(&rel);
                                this.state.select_directory(&rel);
                            } else if let Err(err) = this.state.select_script(&rel, Utc::now()) {
                                eprintln!("select chapter failed: {err:#}");
                            } else {
                                this.invalidate_editor_inputs();
                                this.title_input = None;
                                this.ensure_title_input(window, cx);
                            }
                            cx.notify();
                        }
                    })),
            )
            .into_any_element(),
    );

    if is_dir && expanded {
        for child in &node.children {
            rows.extend(render_node(child, depth + 1, state, cx));
        }
    }
    rows
}

fn build_context_menu(menu: PopupMenu, is_dir: bool) -> PopupMenu {
    if is_dir {
        menu.menu("新建子目录", Box::new(NewDirectory))
            .menu("新建剧本", Box::new(NewScript))
            .separator()
            .menu("重命名", Box::new(RenameNode))
            .separator()
            .menu("删除", Box::new(DeleteNode))
    } else {
        menu.menu("重命名", Box::new(RenameNode))
            .menu("复制", Box::new(CopyScript))
            .separator()
            .menu("上移", Box::new(MoveUp))
            .menu("下移", Box::new(MoveDown))
            .separator()
            .menu("删除", Box::new(DeleteNode))
    }
}
