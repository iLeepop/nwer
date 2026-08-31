//! 章节与剧本的右侧预览面板 UI。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{ActiveTheme as _, v_flex};

use crate::services::{chapter_preview_lines, script_preview_lines};
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

    let is_script = workspace.state.current_script.is_some();

    let mut children: Vec<AnyElement> = vec![
        div()
            .text_sm()
            .font_weight(FontWeight::BOLD)
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
        for (i, line) in lines.into_iter().enumerate() {
            children.push(
                div()
                    .id(SharedString::from(format!("preview-line-{i}")))
                    .text_sm()
                    .line_height(relative(1.7))
                    .when(is_script && line.starts_with('（'), |el| {
                        el.text_color(cx.theme().muted_foreground)
                    })
                    .child(line)
                    .into_any_element(),
            );
        }
    }

    v_flex()
        .id("preview-panel")
        .size_full()
        .p_3()
        .gap_2()
        .bg(cx.theme().background)
        .border_l_1()
        .border_color(cx.theme().border)
        .overflow_y_scroll()
        .children(children)
}
