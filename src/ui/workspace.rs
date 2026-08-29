use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, StyledExt, resizable::h_resizable, resizable::resizable_panel, v_flex,
};

use crate::app::AppState;
use crate::ui::{ai_panel, sidebar, status_bar, top_bar};

pub struct Workspace {
    pub(crate) state: AppState,
}

impl Workspace {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub(crate) fn create_sample_project(&mut self) -> anyhow::Result<()> {
        let stamp = Utc::now().format("%Y%m%d-%H%M%S");
        let title = format!("示例项目-{stamp}");
        self.state.new_project(title, Utc::now())
    }

    pub(crate) fn open_most_recent(&mut self) -> anyhow::Result<()> {
        self.state.open_most_recent(Utc::now())
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chapter_title = self
            .state
            .project
            .as_ref()
            .map(|p| format!("项目：{}", p.title))
            .unwrap_or_else(|| "当前章节标题（未打开项目）".to_string());

        v_flex()
            .id("workspace")
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(top_bar::render_top_bar(&self.state, cx))
            .child(
                div()
                    .id("workspace-body")
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        h_resizable("workspace-panels")
                            .child(
                                resizable_panel()
                                    .visible(self.state.ui.sidebar_visible)
                                    .size(px(240.))
                                    .size_range(px(180.)..px(360.))
                                    .child(sidebar::render_sidebar(&self.state, cx)),
                            )
                            .child(
                                resizable_panel().child(
                                    v_flex()
                                        .id("editor-pane")
                                        .size_full()
                                        .p_4()
                                        .gap_3()
                                        .child(div().text_lg().font_bold().child(chapter_title))
                                        .child(
                                            v_flex()
                                                .flex_1()
                                                .gap_2()
                                                .p_3()
                                                .rounded_md()
                                                .border_1()
                                                .border_color(cx.theme().border)
                                                .bg(cx.theme().muted)
                                                .child("段落块占位")
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(
                                                            "阶段 1：仅布局骨架，块编辑见阶段 2。",
                                                        ),
                                                ),
                                        ),
                                ),
                            )
                            .child(
                                resizable_panel()
                                    .visible(self.state.ui.ai_panel_open)
                                    .size(px(280.))
                                    .size_range(px(220.)..px(420.))
                                    .child(ai_panel::render_ai_panel(&self.state, cx)),
                            ),
                    ),
            )
            .child(status_bar::render_status_bar(&self.state, cx))
    }
}
