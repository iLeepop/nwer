//! 章节与剧本的右侧预览面板 UI。

use gpui::*;
use gpui_component::{ActiveTheme as _, v_flex};

use crate::services::{chapter_preview_lines, script_preview_lines};
use crate::ui::selectable_text::selectable_plain;
use crate::ui::Workspace;

pub fn render_preview_panel(
    workspace: &Workspace,
    _window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let lines: Vec<String> = if let Some(script) = workspace.state.current_script.as_ref() {
        script_preview_lines(script)
    } else if let Some(chapter) = workspace.state.current_chapter.as_ref() {
        chapter_preview_lines(chapter)
    } else {
        Vec::new()
    };

    let mut children: Vec<AnyElement> = vec![
        div()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            .child("预览")
            .into_any_element(),
    ];

    if lines.is_empty() {
        children.push(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("暂无正文可预览")
                .into_any_element(),
        );
    } else {
        let body = lines.join("\n\n");
        children.push(
            selectable_plain("preview-body", body)
                .text_sm()
                .line_height(relative(1.75))
                .text_color(cx.theme().foreground)
                .into_any_element(),
        );
    }

    v_flex()
        .id("preview-panel")
        .size_full()
        .px_4()
        .py_3()
        .gap_3()
        .bg(cx.theme().background)
        .border_l_1()
        .border_color(cx.theme().border)
        .overflow_y_scroll()
        .children(children)
}
