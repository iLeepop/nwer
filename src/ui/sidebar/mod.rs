mod chapter_tree;
mod outline_tree;

use gpui::*;
use gpui_component::{ActiveTheme as _, tab::Tab, tab::TabBar, v_flex};

use crate::app::{AppState, SidebarTab};

pub fn render_sidebar(
    state: &AppState,
    cx: &mut Context<'_, crate::ui::Workspace>,
) -> impl IntoElement {
    let selected = match state.ui.sidebar_tab {
        SidebarTab::Chapters => 0,
        SidebarTab::Outline => 1,
    };

    v_flex()
        .id("sidebar")
        .size_full()
        .gap_2()
        .p_2()
        .border_r_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .child(
            TabBar::new("sidebar-tabs")
                .selected_index(selected)
                .on_click(cx.listener(|this, index, _, cx| {
                    this.state.set_sidebar_tab(match *index {
                        1 => SidebarTab::Outline,
                        _ => SidebarTab::Chapters,
                    });
                    cx.notify();
                }))
                .child(Tab::new().label("章节"))
                .child(Tab::new().label("大纲")),
        )
        .child(
            div()
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .text_color(cx.theme().muted_foreground)
                .child("搜索……"),
        )
        .child(
            v_flex()
                .flex_1()
                .gap_1()
                .p_1()
                .rounded_md()
                .bg(cx.theme().muted)
                .child(match state.ui.sidebar_tab {
                    SidebarTab::Chapters => chapter_tree::placeholder(cx).into_any_element(),
                    SidebarTab::Outline => outline_tree::placeholder(cx).into_any_element(),
                }),
        )
}
