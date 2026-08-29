use gpui::*;
use gpui_component::{ActiveTheme as _, v_flex};

pub fn placeholder(cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .p_2()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("章节树（占位）"),
        )
        .child(div().child("· 卷一 / 开篇"))
        .child(div().child("· 卷一 / 相遇"))
}
