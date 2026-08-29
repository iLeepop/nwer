use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, button::Button, button::ButtonVariants as _,
    h_flex,
};

use crate::app::AppState;
use crate::ui::Workspace;

pub fn render_top_bar(state: &AppState, cx: &mut Context<'_, Workspace>) -> impl IntoElement {
    let title = state.current_title().to_string();
    let dirty_mark = if state.dirty { " ●" } else { "" };
    let project_label = format!("{title}{dirty_mark}");
    let recent_hint = state
        .config
        .recent_projects
        .first()
        .map(|r| format!("打开最近: {}", r.title))
        .unwrap_or_else(|| "打开最近（无）".to_string());
    let has_recent = !state.config.recent_projects.is_empty();
    let ai_label = if state.ui.ai_panel_open {
        "视图·收起 AI"
    } else {
        "视图·展开 AI"
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
            div()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .child(project_label),
        )
        .child(
            Button::new("new-project")
                .primary()
                .small()
                .label("新建示例项目")
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(err) = this.create_sample_project() {
                        eprintln!("new project failed: {err:#}");
                    }
                    cx.notify();
                })),
        )
        .child(
            Button::new("open-recent")
                .small()
                .label(recent_hint)
                .disabled(!has_recent)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Err(err) = this.open_most_recent() {
                        eprintln!("open recent failed: {err:#}");
                    }
                    cx.notify();
                })),
        )
        .child(div().flex_1())
        .child(
            Button::new("save-doc")
                .small()
                .label("保存")
                .disabled(state.current_chapter.is_none())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.save_document(cx);
                })),
        )
        .child(
            h_flex()
                .gap_1()
                .child(menu_placeholder("文件"))
                .child(menu_placeholder("编辑"))
                .child(
                    Button::new("toggle-ai")
                        .ghost()
                        .small()
                        .label(ai_label)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.toggle_ai_panel();
                            cx.notify();
                        })),
                )
                .child(menu_placeholder("设置")),
        )
}

fn menu_placeholder(label: &'static str) -> impl IntoElement {
    Button::new(format!("menu-{label}"))
        .ghost()
        .small()
        .label(label)
}
