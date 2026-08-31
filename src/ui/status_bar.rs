use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex};

use crate::app::AppState;

pub fn render_status_bar(state: &AppState, cx: &App) -> impl IntoElement {
    let err = state.save_error.clone();

    if state.current_script.is_some() {
        let stats = state.current_script_stats();
        return h_flex()
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
            .child(stat("剧本字数", stats.total_words(), cx))
            .child(stat("对话行数", stats.dialogue_count, cx))
            .child(stat("块数", stats.block_count, cx))
            .when(state.dirty, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().warning)
                        .child("未保存"),
                )
            })
            .children(err.map(|e| {
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(format!("保存失败: {e}"))
                    .into_any_element()
            }))
            .into_any_element();
    }

    let stats = state.current_chapter_stats();
    let book_total = state.displayed_book_total();

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
        .child(stat("本章总字数", stats.total_words(), cx))
        .child(stat("汉字", stats.chars.han, cx))
        .child(stat("标点空格", stats.chars.punct_space, cx))
        .child(stat("块数", stats.block_count, cx))
        .child(stat("对话数", stats.dialogue_count, cx))
        .child(stat("全书总字数", book_total, cx))
        .when(state.dirty, |this| {
            this.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().warning)
                    .child("未保存"),
            )
        })
        .children(err.map(|e| {
            div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(format!("保存失败: {e}"))
                .into_any_element()
        }))
        .into_any_element()
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
