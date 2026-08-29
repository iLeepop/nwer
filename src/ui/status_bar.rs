use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex};

use crate::app::AppState;

pub fn render_status_bar(state: &AppState, cx: &App) -> impl IntoElement {
    let book_total = state
        .project
        .as_ref()
        .map(|p| p.total_word_count)
        .unwrap_or(0);

    h_flex()
        .id("status-bar")
        .w_full()
        .px_3()
        .py_1()
        .gap_3()
        .items_center()
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().muted)
        .text_sm()
        .child(stat("本章总字数", 0, cx))
        .child(stat("汉字", 0, cx))
        .child(stat("标点空格", 0, cx))
        .child(stat("块数", 0, cx))
        .child(stat("对话数", 0, cx))
        .child(stat("全书总字数", book_total, cx))
}

fn stat(label: &str, value: u64, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_1()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{label}:")),
        )
        .child(div().child(value.to_string()))
}
