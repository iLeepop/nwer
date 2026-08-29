use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt, WindowExt as _, button::Button,
    dialog::DialogButtonProps, h_flex, input::Input, input::InputState, resizable::h_resizable,
    resizable::resizable_panel, v_flex,
};

use crate::app::AppState;
use crate::storage::{ChapterNodeKind, MoveDirection, find_node_by_rel};
use crate::ui::sidebar::chapter_tree::{
    CopyChapter, DeleteNode, MoveDown, MoveUp, NewChapter, NewDirectory, RenameNode,
};
use crate::ui::{ai_panel, sidebar, status_bar, top_bar};

pub struct Workspace {
    pub(crate) state: AppState,
    title_input: Option<Entity<InputState>>,
}

impl Workspace {
    pub fn new(state: AppState) -> Self {
        Self {
            state,
            title_input: None,
        }
    }

    pub(crate) fn create_sample_project(&mut self) -> anyhow::Result<()> {
        let stamp = Utc::now().format("%Y%m%d-%H%M%S");
        let title = format!("示例项目-{stamp}");
        self.state.new_project(title, Utc::now())?;
        self.title_input = None;
        Ok(())
    }

    pub(crate) fn open_most_recent(&mut self) -> anyhow::Result<()> {
        self.state.open_most_recent(Utc::now())?;
        self.title_input = None;
        Ok(())
    }

    /// 新建目录的父路径：选中目录则用该目录，选中章节则用其父，否则根。
    fn creation_parent(&self) -> String {
        match self.state.ui.selected_node.as_deref() {
            Some(rel) => {
                if let Some(node) = find_node_by_rel(&self.state.chapter_tree, rel)
                    && node.kind == ChapterNodeKind::Directory
                {
                    return rel.to_string();
                }
                parent_of(rel).to_string()
            }
            None => String::new(),
        }
    }

    fn unique_child_name(&self, parent_rel: &str, prefix: &str) -> String {
        let siblings: &[crate::storage::ChapterTreeNode] = if parent_rel.is_empty() {
            self.state.chapter_tree.as_slice()
        } else {
            find_node_by_rel(&self.state.chapter_tree, parent_rel)
                .map(|n| n.children.as_slice())
                .unwrap_or(&[])
        };
        for i in 1..1000 {
            let candidate = format!("{prefix}{i:03}");
            let taken = siblings.iter().any(|c| c.name == candidate);
            if !taken {
                return candidate;
            }
        }
        format!("{prefix}{}", Utc::now().timestamp())
    }

    pub(crate) fn prompt_new_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.project_dir.is_none() {
            return;
        }
        let parent = self.creation_parent();
        let default = self.unique_child_name(&parent, "dir-");
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("目录名")
                .default_value(default)
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            dialog
                .title("新建目录")
                .child(Input::new(&input))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("创建")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            let name = name.trim().to_string();
                            let parent = workspace.read(cx).creation_parent();
                            let new_rel = if parent.is_empty() {
                                name.clone()
                            } else {
                                format!("{parent}/{name}")
                            };
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.create_dir_under(&parent, &name, Utc::now())?;
                                this.state.select_directory(&new_rel);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("create directory failed: {err:#}");
                                return false;
                            }
                            let _ = window;
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_new_chapter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.project_dir.is_none() {
            return;
        }
        let parent = self.creation_parent();
        let default = self.unique_child_name(&parent, "ch-");
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("章节文件名（不含 .json）")
                .default_value(default)
        });
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("章节标题")
                .default_value("未命名章节")
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let name_input = name_input.clone();
            let title_input = title_input.clone();
            let workspace = workspace.clone();
            dialog
                .title("新建章节")
                .child(
                    v_flex()
                        .gap_2()
                        .child(Input::new(&name_input))
                        .child(Input::new(&title_input)),
                )
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("创建")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = name_input.read(cx).value().to_string();
                            let title = title_input.read(cx).value().to_string();
                            let parent = workspace.read(cx).creation_parent();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.create_chapter_under(
                                    &parent,
                                    name.trim(),
                                    title.trim(),
                                    Utc::now(),
                                )?;
                                this.sync_title_input(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("create chapter failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rel) = self.state.ui.selected_node.clone() else {
            return;
        };
        let current_name = find_node_by_rel(&self.state.chapter_tree, &rel)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("新名称")
                .default_value(current_name)
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            let rel = rel.clone();
            dialog
                .title("重命名")
                .child(Input::new(&input))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("确定")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.rename_at(&rel, name.trim(), Utc::now())?;
                                this.sync_title_input(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("rename failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rel) = self.state.ui.selected_node.clone() else {
            return;
        };
        let is_dir = find_node_by_rel(&self.state.chapter_tree, &rel)
            .map(|n| n.kind == ChapterNodeKind::Directory)
            .unwrap_or(false);
        let nonempty = is_dir && self.state.directory_is_nonempty(&rel).unwrap_or(false);
        let description = if nonempty {
            format!("「{rel}」及其所有后代将被永久删除，此操作不可撤销。")
        } else {
            format!("确定删除「{rel}」？")
        };
        let workspace = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let workspace = workspace.clone();
            let rel = rel.clone();
            let description = description.clone();
            alert
                .title("确认删除")
                .description(description)
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    if let Err(err) = workspace.update(cx, |this, cx| {
                        this.state.delete_at(&rel, Utc::now())?;
                        this.title_input = None;
                        cx.notify();
                        anyhow::Ok(())
                    }) {
                        eprintln!("delete failed: {err:#}");
                    }
                    true
                })
        });
    }

    pub(crate) fn copy_selected_chapter(&mut self) {
        let Some(rel) = self.state.ui.selected_node.clone() else {
            return;
        };
        let Some(node) = find_node_by_rel(&self.state.chapter_tree, &rel).cloned() else {
            return;
        };
        if node.kind != ChapterNodeKind::Chapter {
            return;
        }
        let parent = parent_of(&rel).to_string();
        let new_name = format!("{}-副本", node.name);
        if let Err(err) = self
            .state
            .copy_chapter_at(&rel, &parent, &new_name, Utc::now())
        {
            eprintln!("copy chapter failed: {err:#}");
        }
    }

    fn sync_title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(chapter) = self.state.current_chapter.as_ref() {
            let title = chapter.title.clone();
            match &self.title_input {
                Some(input) => {
                    input.update(cx, |state, cx| {
                        state.set_value(title, window, cx);
                    });
                }
                None => {
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("章节标题")
                            .default_value(title)
                    });
                    self.title_input = Some(input);
                }
            }
        } else {
            self.title_input = None;
        }
    }

    fn ensure_title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.current_chapter.is_some() && self.title_input.is_none() {
            self.sync_title_input(window, cx);
        }
        if self.state.current_chapter.is_none() {
            self.title_input = None;
        }
    }
}

fn parent_of(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_title_input(window, cx);

        let chapter_title = self
            .state
            .current_chapter
            .as_ref()
            .map(|c| c.title.clone())
            .unwrap_or_else(|| {
                if self.state.project.is_some() {
                    "选择一个章节".to_string()
                } else {
                    "当前章节标题（未打开项目）".to_string()
                }
            });

        let blocks = self
            .state
            .current_chapter
            .as_ref()
            .map(|c| c.blocks.clone())
            .unwrap_or_default();

        v_flex()
            .id("workspace")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &NewDirectory, window, cx| {
                this.prompt_new_directory(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewChapter, window, cx| {
                this.prompt_new_chapter(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RenameNode, window, cx| {
                this.prompt_rename(window, cx);
            }))
            .on_action(cx.listener(|this, _: &DeleteNode, window, cx| {
                this.confirm_delete(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CopyChapter, _, cx| {
                this.copy_selected_chapter();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| {
                if let Err(err) = this
                    .state
                    .move_selected_sibling(MoveDirection::Up, Utc::now())
                {
                    eprintln!("move up failed: {err:#}");
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| {
                if let Err(err) = this
                    .state
                    .move_selected_sibling(MoveDirection::Down, Utc::now())
                {
                    eprintln!("move down failed: {err:#}");
                }
                cx.notify();
            }))
            .child(top_bar::render_top_bar(&self.state, cx))
            .child(
                div()
                    .id("workspace-body")
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        h_resizable("workspace-panels")
                            .child(
                                resizable_panel()
                                    .visible(self.state.ui.sidebar_visible)
                                    .size(px(240.))
                                    .size_range(px(180.)..px(360.))
                                    .child(sidebar::render_sidebar(&self.state, cx)),
                            )
                            .child(
                                resizable_panel().child(
                                    v_flex()
                                        .id("editor-pane")
                                        .size_full()
                                        .p_4()
                                        .gap_3()
                                        .child(if let Some(input) = self.title_input.as_ref() {
                                            div()
                                                .w_full()
                                                .child(Input::new(input).into_any_element())
                                                .into_any_element()
                                        } else {
                                            div()
                                                .text_lg()
                                                .font_bold()
                                                .child(chapter_title)
                                                .into_any_element()
                                        })
                                        .child(
                                            h_flex().gap_2().child(
                                                Button::new("save-title")
                                                    .small()
                                                    .label("保存标题")
                                                    .disabled(self.title_input.is_none())
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            if let Some(input) =
                                                                this.title_input.clone()
                                                            {
                                                                let title = input
                                                                    .read(cx)
                                                                    .value()
                                                                    .to_string();
                                                                if let Err(err) = this
                                                                    .state
                                                                    .set_current_chapter_title(
                                                                        title.trim(),
                                                                    )
                                                                {
                                                                    eprintln!(
                                                                        "save title failed: {err:#}"
                                                                    );
                                                                }
                                                                this.sync_title_input(window, cx);
                                                                cx.notify();
                                                            }
                                                        },
                                                    )),
                                            ),
                                        )
                                        .child(
                                            v_flex()
                                                .flex_1()
                                                .gap_2()
                                                .p_3()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(cx.theme().border)
                                                .bg(cx.theme().muted)
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(if blocks.is_empty() {
                                                            "暂无段落块（块编辑见下一阶段）"
                                                                .to_string()
                                                        } else {
                                                            format!("共 {} 个段落块", blocks.len())
                                                        }),
                                                )
                                                .children(
                                                    blocks.into_iter().take(12).enumerate().map(
                                                        |(i, block)| {
                                                            let preview =
                                                                if block.content.is_empty() {
                                                                    "（空）".to_string()
                                                                } else {
                                                                    let t = block
                                                                        .content
                                                                        .replace('\n', " ");
                                                                    if t.chars().count() > 40 {
                                                                        format!(
                                                                    "{}…",
                                                                    t.chars()
                                                                        .take(40)
                                                                        .collect::<String>()
                                                                )
                                                                    } else {
                                                                        t
                                                                    }
                                                                };
                                                            div().text_sm().child(format!(
                                                                "{}. [{}] {}",
                                                                i + 1,
                                                                block.block_type.label(),
                                                                preview
                                                            ))
                                                        },
                                                    ),
                                                ),
                                        ),
                                ),
                            )
                            .child(
                                resizable_panel()
                                    .visible(self.state.ui.ai_panel_open)
                                    .size(px(280.))
                                    .size_range(px(220.)..px(420.))
                                    .child(ai_panel::render_ai_panel(&self.state, cx)),
                            ),
                    ),
            )
            .child(status_bar::render_status_bar(&self.state, cx))
    }
}
