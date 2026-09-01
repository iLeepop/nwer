use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex};

use crate::app::AppState;
use crate::ui::manuscript::{StatTier, chapter_stat_tier};

pub fn render_status_bar(state: &AppState, cx: &App) -> impl IntoElement {
    let err = state.save_error.clone();

    if state.current_script.is_some() {
        let stats = state.current_script_stats();
        return h_flex()
            .id("status-bar")
            .w_full()
            .px_3()
            .py_1()
            .gap_4()
            .items_baseline()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(stat("剧本字数", stats.total_words(), cx))
            .child(stat("对话行数", stats.dialogue_count, cx))
            .child(stat("块数", stats.block_count, cx))
            .child(div().flex_1())
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
        .gap_4()
        .items_baseline()
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .child(stat("本章总字数", stats.total_words(), cx))
        .child(stat("全书总字数", book_total, cx))
        .child(stat("汉字", stats.chars.han, cx))
        .child(stat("块数", stats.block_count, cx))
        .child(stat("对话数", stats.dialogue_count, cx))
        .child(div().flex_1())
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
    let tier = chapter_stat_tier(label);
    match tier {
        StatTier::Hero => h_flex()
            .gap_1p5()
            .items_baseline()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().foreground)
                    .child(value.to_string()),
            )
            .into_any_element(),
        StatTier::Secondary => h_flex()
            .gap_1()
            .items_baseline()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(cx.theme().foreground)
                    .child(value.to_string()),
            )
            .into_any_element(),
        StatTier::Meta => h_flex()
            .gap_1()
            .items_baseline()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.85))
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(value.to_string()),
            )
            .into_any_element(),
    }
}
