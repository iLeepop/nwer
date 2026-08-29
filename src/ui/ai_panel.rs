use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt, h_flex, v_flex};

use crate::app::AppState;

/// AI 助手面板；折叠时由外层 `resizable_panel().visible(false)` 隐藏。
pub fn render_ai_panel(_state: &AppState, cx: &App) -> impl IntoElement {
    v_flex()
        .id("ai-panel")
        .size_full()
        .p_3()
        .gap_2()
        .bg(cx.theme().background)
        .child(div().font_bold().child("AI 助手"))
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("第一版仅提供占位界面，后续接入 Agent library。"),
        )
        .child(
            v_flex()
                .flex_1()
                .p_2()
                .rounded_md()
                .bg(cx.theme().muted)
                .child("这里将显示对话消息流。"),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().border)
                        .text_color(cx.theme().muted_foreground)
                        .child("输入框（已禁用）"),
                )
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(cx.theme().muted)
                        .text_color(cx.theme().muted_foreground)
                        .child("发送"),
                ),
        )
}
