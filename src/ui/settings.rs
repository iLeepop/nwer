//! 设置对话框：左侧垂直导航（通用 / 项目 / AI）+ 右侧表单。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IndexPath, WindowExt as _, dialog::DialogButtonProps, h_flex, input::Input,
    input::InputState, input::Textarea, input::TextareaState, select::Select, select::SelectEvent,
    select::SelectState, v_flex,
};

use crate::ui::dialog_ok_cancel_footer;

use chrono::Utc;

use crate::models::AiContext;
use crate::storage::{
    AiSettings, ai_providers, clamp_max_tool_rounds, default_base_url_for_provider,
    validate_settings_save,
};
use crate::ui::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SettingsTab {
    #[default]
    General,
    Project,
    Ai,
}

pub(crate) struct SettingsView {
    tab: SettingsTab,
    root_input: Entity<InputState>,
    depth_input: Entity<InputState>,
    writer_prompt_input: Entity<TextareaState>,
    reviewer_prompt_input: Entity<TextareaState>,
    director_prompt_input: Entity<TextareaState>,
    provider_select: Entity<SelectState<Vec<SharedString>>>,
    api_key_input: Entity<InputState>,
    base_url_input: Entity<InputState>,
    model_input: Entity<InputState>,
    max_tool_rounds_input: Entity<InputState>,
    has_project: bool,
    _provider_sub: Option<Subscription>,
}

impl SettingsView {
    pub(crate) fn new(
        root: String,
        depth: String,
        ai: AiSettings,
        project_ai: AiContext,
        has_project: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let root_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("项目根目录")
                .default_value(root)
        });
        let depth_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("当前项目 max_depth")
                .default_value(depth)
        });

        let writer_prompt_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 10)
                .placeholder("追加到系统「写手」提示词之后")
                .default_value(project_ai.writer_prompt)
        });
        let reviewer_prompt_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 10)
                .placeholder("追加到系统「审查」提示词之后")
                .default_value(project_ai.reviewer_prompt)
        });
        let director_prompt_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 10)
                .placeholder("追加到系统「导演」提示词之后")
                .default_value(project_ai.director_prompt)
        });

        let labels: Vec<SharedString> = ai_providers()
            .iter()
            .map(|(_, label)| SharedString::from(*label))
            .collect();
        let selected_ix = ai_providers()
            .iter()
            .position(|(id, _)| *id == ai.provider.as_str())
            .unwrap_or(0);
        let provider_select = cx.new(|cx| {
            SelectState::new(
                labels,
                Some(IndexPath::default().row(selected_ix)),
                window,
                cx,
            )
        });

        let api_key_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("API Key")
                .default_value(ai.api_key.clone())
                .masked(true)
        });
        let base_url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("默认调用地址")
                .default_value(ai.base_url.clone())
        });
        let model_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("模型 id")
                .default_value(ai.model.clone())
        });
        let max_tool_rounds_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("工具循环轮次 1–64")
                .default_value(ai.max_tool_rounds.to_string())
        });

        let mut view = Self {
            tab: SettingsTab::General,
            root_input,
            depth_input,
            writer_prompt_input,
            reviewer_prompt_input,
            director_prompt_input,
            provider_select: provider_select.clone(),
            api_key_input,
            base_url_input: base_url_input.clone(),
            model_input,
            max_tool_rounds_input,
            has_project,
            _provider_sub: None,
        };

        view._provider_sub = Some(cx.subscribe_in(&provider_select, window, {
            move |this, select, event, window, cx| {
                let SelectEvent::Confirm(Some(label)) = event else {
                    return;
                };
                let id = ai_providers()
                    .iter()
                    .find(|(_, name)| *name == label.as_ref())
                    .map(|(id, _)| *id);
                let Some(id) = id else {
                    return;
                };
                let Some(url) = default_base_url_for_provider(id) else {
                    return;
                };
                this.base_url_input.update(cx, |state, cx| {
                    state.set_value(url, window, cx);
                });
                let _ = select;
            }
        }));

        view
    }

    fn collect_ai(&self, cx: &App) -> AiSettings {
        let label = self
            .provider_select
            .read(cx)
            .selected_value()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "DeepSeek".into());
        let provider = ai_providers()
            .iter()
            .find(|(_, name)| *name == label.as_str())
            .map(|(id, _)| (*id).to_string())
            .unwrap_or_else(|| "deepseek".into());
        let rounds_raw = self.max_tool_rounds_input.read(cx).value().to_string();
        let max_tool_rounds = rounds_raw
            .trim()
            .parse::<u32>()
            .map(clamp_max_tool_rounds)
            .unwrap_or_else(|_| crate::storage::default_max_tool_rounds());
        AiSettings {
            provider,
            api_key: self.api_key_input.read(cx).value().to_string(),
            base_url: self.base_url_input.read(cx).value().to_string(),
            model: self.model_input.read(cx).value().to_string(),
            max_tool_rounds,
        }
    }

    fn collect_role_prompts(&self, cx: &App) -> (String, String, String) {
        (
            self.writer_prompt_input.read(cx).value().to_string(),
            self.reviewer_prompt_input.read(cx).value().to_string(),
            self.director_prompt_input.read(cx).value().to_string(),
        )
    }

    /// Left-aligned vertical nav row (not a centered Button).
    fn nav_item(
        &self,
        id: &'static str,
        label: &'static str,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = self.tab == tab;
        div()
            .id(id)
            .w_full()
            .px_2p5()
            .py_1p5()
            .rounded(cx.theme().radius)
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(cx.theme().accent.opacity(0.14))
                    .text_color(cx.theme().foreground)
            })
            .when(!selected, |this| {
                this.text_color(cx.theme().muted_foreground).hover(|this| {
                    this.bg(cx.theme().muted.opacity(0.7))
                        .text_color(cx.theme().foreground)
                })
            })
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .when(selected, |this| this.font_weight(FontWeight::MEDIUM))
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
    }

    fn field_label(label: &'static str, cx: &App) -> impl IntoElement {
        div()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let right = match self.tab {
            SettingsTab::General => v_flex()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("通用"),
                )
                .child(Self::field_label("项目根目录 (projects_root)", cx))
                .child(Input::new(&self.root_input))
                .child(Self::field_label("当前项目最大层级 (max_depth)", cx))
                .child(Input::new(&self.depth_input).disabled(!self.has_project))
                .when(!self.has_project, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("未打开项目时不修改 max_depth"),
                    )
                })
                .into_any_element(),
            SettingsTab::Project => v_flex()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("项目"),
                )
                .when(!self.has_project, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("打开项目后可编辑写手 / 审查 / 导演提示词"),
                    )
                })
                .child(Self::field_label("写手提示词", cx))
                .child(
                    Textarea::new(&self.writer_prompt_input).disabled(!self.has_project),
                )
                .child(Self::field_label("审查提示词", cx))
                .child(
                    Textarea::new(&self.reviewer_prompt_input).disabled(!self.has_project),
                )
                .child(Self::field_label("导演提示词", cx))
                .child(
                    Textarea::new(&self.director_prompt_input).disabled(!self.has_project),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("选择对应 Agent 时追加到系统角色提示词之后；留空则只用系统默认段"),
                )
                .into_any_element(),
            SettingsTab::Ai => v_flex()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("AI"),
                )
                .child(Self::field_label("模型提供商", cx))
                .child(Select::new(&self.provider_select).placeholder("选择提供商"))
                .child(Self::field_label("API Key", cx))
                .child(Input::new(&self.api_key_input).mask_toggle())
                .child(Self::field_label("默认调用地址", cx))
                .child(Input::new(&self.base_url_input))
                .child(Self::field_label("模型", cx))
                .child(Input::new(&self.model_input))
                .child(Self::field_label("工具循环轮次 (max_tool_rounds)", cx))
                .child(Input::new(&self.max_tool_rounds_input))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("单次对话最多工具轮次，范围 1–64，默认 24"),
                )
                .into_any_element(),
        };

        h_flex()
            .id("settings-dual-pane")
            .w_full()
            .min_h(px(360.))
            .items_start()
            .child(
                v_flex()
                    .id("settings-nav")
                    .w(px(148.))
                    .flex_shrink_0()
                    .justify_start()
                    .items_stretch()
                    .gap_0p5()
                    .pr_3()
                    .mr_3()
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(self.nav_item(
                        "settings-tab-general",
                        "通用",
                        SettingsTab::General,
                        cx,
                    ))
                    .child(self.nav_item(
                        "settings-tab-project",
                        "项目",
                        SettingsTab::Project,
                        cx,
                    ))
                    .child(self.nav_item("settings-tab-ai", "AI", SettingsTab::Ai, cx)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .justify_start()
                    .pt_0p5()
                    .child(right),
            )
    }
}

/// 打开设置 Dialog（由 Workspace 委托）。
///
/// `root`/`depth`/`ai`/`project_ai`/`has_project` 须由调用方在已持有 `&mut Workspace` 时取出。
pub(crate) fn open_settings_dialog(
    workspace: Entity<Workspace>,
    root: String,
    depth: String,
    ai: AiSettings,
    project_ai: AiContext,
    has_project: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let settings =
        cx.new(|cx| SettingsView::new(root, depth, ai, project_ai, has_project, window, cx));

    window.open_dialog(cx, move |dialog, _, _| {
        let settings = settings.clone();
        let workspace = workspace.clone();
        dialog
            .title("设置")
            .w(px(720.))
            .child(settings.clone())
            .footer(dialog_ok_cancel_footer("保存", "取消"))
            .button_props(DialogButtonProps::default().on_ok(move |_, _, cx| {
                let root = settings.read(cx).root_input.read(cx).value().to_string();
                let depth = settings.read(cx).depth_input.read(cx).value().to_string();
                let ai = settings.read(cx).collect_ai(cx);
                let (writer, reviewer, director) = settings.read(cx).collect_role_prompts(cx);
                if let Err(err) = validate_settings_save(root.trim(), ai.base_url.trim()) {
                    eprintln!("settings validation failed: {err:#}");
                    return false;
                }
                if let Err(err) = workspace.update(cx, |this, cx| {
                    this.state.set_projects_root(root.trim())?;
                    this.state.set_ai_settings(ai)?;
                    if this.state.project.is_some() {
                        let d: u32 = depth.trim().parse().unwrap_or(3);
                        this.state.set_max_depth(d, Utc::now())?;
                        this.state
                            .set_project_role_prompts(writer, reviewer, director, Utc::now())?;
                    }
                    cx.notify();
                    anyhow::Ok(())
                }) {
                    eprintln!("settings failed: {err:#}");
                    return false;
                }
                true
            }))
    });
}
