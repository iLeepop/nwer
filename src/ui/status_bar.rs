use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Selectable as _, Sizable as _, button::Button,
    button::ButtonVariants as _, h_flex,
};

use crate::app::AppState;
use crate::ui::Workspace;
use crate::ui::manuscript::{StatTier, chapter_stat_tier};

pub fn render_status_bar(
    state: &AppState,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let err = state.save_error.clone();

    if state.current_script.is_some() {
        let stats = state.current_script_stats();
        return h_flex()
            .id("status-bar")
            .w_full()
            .px_2()
            .py_1()
            .gap_3()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(render_sidebar_toggle(state, cx))
            .child(stat("剧本字数", stats.total_words(), cx))
            .child(stat("对话行数", stats.dialogue_count, cx))
            .child(stat("块数", stats.block_count, cx))
            .child(div().flex_1())
            .children(status_messages(state, err, cx))
            .child(render_ai_toggle(state, cx))
            .into_any_element();
    }

    let stats = state.current_chapter_stats();

    h_flex()
        .id("status-bar")
        .w_full()
        .px_2()
        .py_1()
        .gap_3()
        .items_center()
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .child(render_sidebar_toggle(state, cx))
        .child(stat("本章总字数", stats.total_words(), cx))
        .child(stat("汉字", stats.chars.han, cx))
        .child(stat("块数", stats.block_count, cx))
        .child(stat("对话数", stats.dialogue_count, cx))
        .child(div().flex_1())
        .children(status_messages(state, err, cx))
        .child(render_ai_toggle(state, cx))
        .into_any_element()
}

fn render_sidebar_toggle(
    state: &AppState,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let visible = state.ui.sidebar_visible;
    let icon = if visible {
        IconName::PanelLeftClose
    } else {
        IconName::PanelLeftOpen
    };
    let tooltip = if visible {
        "隐藏侧边栏（章节 / 大纲 / 剧本）"
    } else {
        "显示侧边栏（章节 / 大纲 / 剧本）"
    };

    Button::new("status-toggle-sidebar")
        .ghost()
        .xsmall()
        .icon(icon)
        .tooltip(tooltip)
        .selected(visible)
        .on_click(cx.listener(|this, _, _, cx| {
            this.state.toggle_sidebar();
            cx.notify();
        }))
}

fn render_ai_toggle(state: &AppState, cx: &mut Context<'_, Workspace>) -> impl IntoElement {
    let open = state.ui.ai_panel_open;
    let icon = if open {
        IconName::PanelRightClose
    } else {
        IconName::PanelRightOpen
    };
    let tooltip = if open { "隐藏 AI 面板" } else { "显示 AI 面板" };

    Button::new("status-toggle-ai")
        .ghost()
        .xsmall()
        .icon(icon)
        .tooltip(tooltip)
        .selected(open)
        .on_click(cx.listener(|this, _, _, cx| {
            this.state.toggle_ai_panel();
            cx.notify();
        }))
}

fn status_messages(
    state: &AppState,
    err: Option<String>,
    cx: &App,
) -> Option<AnyElement> {
    if !state.dirty && err.is_none() {
        return None;
    }

    Some(
        h_flex()
            .gap_3()
            .items_center()
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
            .into_any_element(),
    )
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
