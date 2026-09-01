//! 拖到 AI 面板的统一载荷。

use gpui::*;
use gpui_component::ActiveTheme as _;

use crate::ai::AiContextRef;

/// 侧栏树 / 大纲拖到 AI 输入坞的载荷。
#[derive(Clone)]
pub struct AiDragPayload {
    pub context: AiContextRef,
}

impl Render for AiDragPayload {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .text_xs()
            .bg(cx.theme().accent)
            .text_color(cx.theme().accent_foreground)
            .rounded_sm()
            .child(format!(
                "{} · {}",
                self.context.kind.label(),
                self.context.title
            ))
    }
}
