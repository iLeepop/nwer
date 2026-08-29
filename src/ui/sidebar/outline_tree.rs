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
                .child("大纲树（占位）"),
        )
        .child(div().child("· 角色"))
        .child(div().child("· 背景"))
        .child(div().child("· 场景"))
        .child(div().child("· 事件"))
        .child(div().child("· 杂项"))
}
