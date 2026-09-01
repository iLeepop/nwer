use gpui::*;
use gpui_component::{
    ActiveTheme as _, Sizable as _, button::Button, button::ButtonVariants as _, h_flex,
    menu::DropdownMenu,
};
use serde::Deserialize;

use crate::app::AppState;
use crate::ui::Workspace;
use crate::ui::editor::SaveDocument;

actions!(
    top_bar_menu,
    [
        NewProject,
        OpenProject,
        QuitApp,
        ToggleSidebar,
        ToggleAiPanel,
        OpenSettings,
    ]
);

/// 打开最近列表第 N 项。
#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = nwer, no_json)]
pub struct OpenRecentAt(pub usize);

pub fn render_top_bar(state: &AppState, cx: &mut Context<'_, Workspace>) -> impl IntoElement {
    let title = state.current_title().to_string();
    let dirty_mark = if state.dirty { " ●" } else { "" };
    let project_label = format!("{title}{dirty_mark}");
    let can_save = state.current_chapter.is_some() || state.current_outline.is_some();
    let recent = state.config.recent_projects.clone();
    let sidebar_label = if state.ui.sidebar_visible {
        "隐藏左栏"
    } else {
        "显示左栏"
    };
    let ai_label = if state.ui.ai_panel_open {
        "隐藏 AI 面板"
    } else {
        "显示 AI 面板"
    };

    h_flex()
        .id("top-bar")
        .w_full()
        .px_3()
        .py_2()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .child(
            Button::new("project-dropdown")
                .small()
                .label(project_label)
                .dropdown_menu({
                    let recent = recent.clone();
                    move |mut menu, _, _| {
                        if recent.is_empty() {
                            menu = menu.menu("最近项目（无）", Box::new(OpenProject));
                        } else {
                            for (i, item) in recent.iter().take(10).enumerate() {
                                menu = menu.menu(
                                    format!("{} — {}", item.title, item.path),
                                    Box::new(OpenRecentAt(i)),
                                );
                            }
                        }
                        menu
                    }
                }),
        )
        .child(
            Button::new("new-project")
                .primary()
                .small()
                .label("新建")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.prompt_new_project(window, cx);
                })),
        )
        .child(
            Button::new("open-project")
                .small()
                .label("打开")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.prompt_open_project(window, cx);
                })),
        )
        .child(div().flex_1())
        .child(
            h_flex()
                .gap_1()
                .child(file_menu(can_save))
                .child(view_menu(sidebar_label, ai_label))
                .child(
                    Button::new("menu-settings")
                        .ghost()
                        .small()
                        .label("设置")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.prompt_settings(window, cx);
                        })),
                ),
        )
}

fn file_menu(can_save: bool) -> impl IntoElement {
    Button::new("menu-file")
        .ghost()
        .small()
        .label("文件")
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu
                .menu("新建项目", Box::new(NewProject))
                .menu("打开项目", Box::new(OpenProject));
            if can_save {
                menu = menu.menu("保存", Box::new(SaveDocument));
            }
            menu.separator().menu("退出", Box::new(QuitApp))
        })
}

fn view_menu(sidebar_label: &'static str, ai_label: &'static str) -> impl IntoElement {
    Button::new("menu-view")
        .ghost()
        .small()
        .label("视图")
        .dropdown_menu(move |menu, _, _| {
            menu.menu(sidebar_label, Box::new(ToggleSidebar))
                .menu(ai_label, Box::new(ToggleAiPanel))
        })
}
