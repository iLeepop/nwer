//! 设置对话框：左栏「通用 / AI」双栏布局。

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IndexPath, Selectable as _, StyledExt, WindowExt as _,
    button::Button, dialog::DialogButtonProps, h_flex, input::Input, input::InputState,
    select::Select, select::SelectEvent, select::SelectState, v_flex,
};

use chrono::Utc;

use crate::storage::{
    AiSettings, ai_providers, default_base_url_for_provider, validate_settings_save,
};
use crate::ui::Workspace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SettingsTab {
    #[default]
    General,
    Ai,
}

pub(crate) struct SettingsView {
    tab: SettingsTab,
    root_input: Entity<InputState>,
    depth_input: Entity<InputState>,
    provider_select: Entity<SelectState<Vec<SharedString>>>,
    api_key_input: Entity<InputState>,
    base_url_input: Entity<InputState>,
    model_input: Entity<InputState>,
    has_project: bool,
    _provider_sub: Option<Subscription>,
}

impl SettingsView {
    pub(crate) fn new(
        root: String,
        depth: String,
        ai: AiSettings,
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

        let mut view = Self {
            tab: SettingsTab::General,
            root_input,
            depth_input,
            provider_select: provider_select.clone(),
            api_key_input,
            base_url_input: base_url_input.clone(),
            model_input,
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
                // silence unused if select unused
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
        AiSettings {
            provider,
            api_key: self.api_key_input.read(cx).value().to_string(),
            base_url: self.base_url_input.read(cx).value().to_string(),
            model: self.model_input.read(cx).value().to_string(),
        }
    }

    fn nav_item(
        &self,
        id: &'static str,
        label: &'static str,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        Button::new(id)
            .label(label)
            .selected(self.tab == tab)
            .w_full()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.tab = tab;
                cx.notify();
            }))
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let right = match self.tab {
            SettingsTab::General => v_flex()
                .gap_2()
                .flex_1()
                .child(div().font_bold().child("通用"))
                .child(div().text_sm().child("项目根目录 (projects_root)"))
                .child(Input::new(&self.root_input))
                .child(div().text_sm().child("当前项目最大层级 (max_depth)"))
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
            SettingsTab::Ai => v_flex()
                .gap_2()
                .flex_1()
                .child(div().font_bold().child("AI"))
                .child(div().text_sm().child("模型提供商"))
                .child(Select::new(&self.provider_select).placeholder("选择提供商"))
                .child(div().text_sm().child("API Key"))
                .child(Input::new(&self.api_key_input).mask_toggle())
                .child(div().text_sm().child("默认调用地址"))
                .child(Input::new(&self.base_url_input))
                .child(div().text_sm().child("模型"))
                .child(Input::new(&self.model_input))
                .into_any_element(),
        };

        h_flex()
            .id("settings-dual-pane")
            .w(px(680.))
            .min_h(px(320.))
            .gap_3()
            .child(
                v_flex()
                    .w(px(120.))
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().muted)
                    .child(self.nav_item("settings-tab-general", "通用", SettingsTab::General, cx))
                    .child(self.nav_item("settings-tab-ai", "AI", SettingsTab::Ai, cx)),
            )
            .child(v_flex().flex_1().p_1().child(right))
    }
}

/// 打开设置 Dialog（由 Workspace 委托）。
pub(crate) fn open_settings_dialog(
    workspace: Entity<Workspace>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let (root, depth, ai, has_project) = {
        let this = workspace.read(cx);
        (
            this.state.config.projects_root.clone(),
            this.state.max_depth().to_string(),
            this.state.ai_settings().clone(),
            this.state.project.is_some(),
        )
    };

    let settings = cx.new(|cx| SettingsView::new(root, depth, ai, has_project, window, cx));

    window.open_dialog(cx, move |dialog, _, _| {
        let settings = settings.clone();
        let workspace = workspace.clone();
        dialog
            .title("设置")
            .w(px(720.))
            .child(settings.clone())
            .button_props(
                DialogButtonProps::default()
                    .ok_text("保存")
                    .show_cancel(true)
                    .cancel_text("取消")
                    .on_ok(move |_, _, cx| {
                        let root = settings.read(cx).root_input.read(cx).value().to_string();
                        let depth = settings.read(cx).depth_input.read(cx).value().to_string();
                        let ai = settings.read(cx).collect_ai(cx);
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
                            }
                            cx.notify();
                            anyhow::Ok(())
                        }) {
                            eprintln!("settings failed: {err:#}");
                            return false;
                        }
                        true
                    }),
            )
    });
}
