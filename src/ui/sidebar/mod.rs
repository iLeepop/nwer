pub(crate) mod chapter_tree;
pub(crate) mod outline_tree;

use gpui::*;
use gpui_component::{
    ActiveTheme as _, Selectable as _, Sizable as _, button::Button, button::ButtonVariants as _,
    h_flex, input::Input, tab::Tab, tab::TabBar, v_flex,
};

use crate::app::SidebarTab;
use crate::services::SearchMode;
use crate::ui::Workspace;

pub fn render_sidebar(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    workspace.ensure_search_input(window, cx);
    let state = &workspace.state;
    let selected = match state.ui.sidebar_tab {
        SidebarTab::Chapters => 0,
        SidebarTab::Outline => 1,
    };
    let mode = state.search_mode;
    let is_full_text = mode == SearchMode::FullText;
    let name_selected = mode == SearchMode::NameFilter;
    let full_selected = mode == SearchMode::FullText;

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
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            Button::new("search-mode-name")
                                .xsmall()
                                .label("名称")
                                .selected(name_selected)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Err(err) =
                                        this.state.set_search_mode(SearchMode::NameFilter)
                                    {
                                        eprintln!("set search mode failed: {err:#}");
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("search-mode-full")
                                .xsmall()
                                .label("全文")
                                .selected(full_selected)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Err(err) =
                                        this.state.set_search_mode(SearchMode::FullText)
                                    {
                                        eprintln!("set search mode failed: {err:#}");
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(if let Some(input) = workspace.search_input.as_ref() {
                    Input::new(input).into_any_element()
                } else {
                    div().child("搜索……").into_any_element()
                }),
        )
        .child(
            v_flex()
                .flex_1()
                .gap_1()
                .p_1()
                .rounded_md()
                .bg(cx.theme().muted)
                .child(if is_full_text {
                    render_full_text_hits(workspace, cx).into_any_element()
                } else {
                    match workspace.state.ui.sidebar_tab {
                        SidebarTab::Chapters => {
                            chapter_tree::render_chapter_tree(&workspace.state, cx)
                                .into_any_element()
                        }
                        SidebarTab::Outline => {
                            outline_tree::render_outline_tree(&workspace.state, cx)
                                .into_any_element()
                        }
                    }
                }),
        )
}

fn render_full_text_hits(
    workspace: &Workspace,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let hits = &workspace.state.full_text_hits;
    v_flex()
        .id("full-text-hits")
        .size_full()
        .gap_1()
        .overflow_y_scroll()
        .children(if hits.is_empty() {
            vec![
                div()
                    .p_2()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(if workspace.state.search_query.trim().is_empty() {
                        "输入关键词进行全文搜索"
                    } else {
                        "无匹配结果"
                    })
                    .into_any_element(),
            ]
        } else {
            hits.iter()
                .enumerate()
                .map(|(index, hit)| {
                    let type_label = hit.block_type.label();
                    let title = hit.chapter_title.clone();
                    let snippet = hit.snippet.clone();
                    Button::new(format!("hit-{index}"))
                        .ghost()
                        .w_full()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Err(err) =
                                this.state.open_full_text_hit(index, chrono::Utc::now())
                            {
                                eprintln!("open hit failed: {err:#}");
                            }
                            this.invalidate_editor_inputs();
                            this.title_input = None;
                            this.invalidate_outline_inputs();
                            this.ensure_title_input(window, cx);
                            cx.notify();
                        }))
                        .child(
                            v_flex()
                                .gap_0()
                                .w_full()
                                .items_start()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!("{title} · {type_label}")),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(snippet),
                                ),
                        )
                        .into_any_element()
                })
                .collect()
        })
}
