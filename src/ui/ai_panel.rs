use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, StyledExt, button::Button,
    button::ButtonVariants as _, h_flex, input::Input, v_flex,
};

use crate::ai::{summarize_intent, AiChatRole, AiMaxTokenTier};
use crate::ui::Workspace;

/// AI 助手面板：自动应用开关、消息流、提案确认、输入发送。
pub fn render_ai_panel(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    workspace.ensure_ai_input(window, cx);

    let auto_apply = workspace.state.ai.auto_apply;
    let busy = workspace.state.ai.busy;
    let max_token_tier = workspace.state.ai.max_token_tier;
    let auto_label = if auto_apply {
        "自动应用：开"
    } else {
        "自动应用：关"
    };
    let status = workspace.state.ai.status_message.clone();
    let messages: Vec<(AiChatRole, String)> = workspace
        .state
        .ai
        .messages
        .iter()
        .map(|m| (m.role, m.text.clone()))
        .collect();
    let proposals: Vec<(uuid::Uuid, String, bool)> = workspace
        .state
        .ai
        .proposals
        .iter()
        .map(|p| {
            let summary = summarize_intent(&p.intent);
            let label = if p.stale {
                format!("{summary}（已过期）")
            } else {
                summary
            };
            (p.intent.intent_id(), label, p.stale)
        })
        .collect();
    let has_proposals = !proposals.is_empty();
    let ai_input = workspace.ai_input.clone();

    v_flex()
        .id("ai-panel")
        .size_full()
        .p_3()
        .gap_2()
        .bg(cx.theme().background)
        .border_l_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().font_bold().child("AI 助手"))
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(
                            h_flex()
                                .gap_0()
                                .children(AiMaxTokenTier::all().map(|tier| {
                                    let selected = max_token_tier == tier;
                                    Button::new(format!("ai-max-token-{}", tier.label()))
                                        .xsmall()
                                        .label(tier.label())
                                        .selected(selected)
                                        .disabled(busy)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.state.set_ai_max_token_tier(tier);
                                            cx.notify();
                                        }))
                                })),
                        )
                        .child(
                            Button::new("ai-auto-apply")
                                .small()
                                .label(auto_label)
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.toggle_ai_auto_apply();
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .children(status.map(|msg| {
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(msg)
        }))
        .child(
            v_flex()
                .id("ai-messages")
                .flex_1()
                .gap_1()
                .p_2()
                .rounded_md()
                .bg(cx.theme().muted)
                .overflow_y_scroll()
                .children(if messages.is_empty() {
                    vec![div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("还没有对话。在下方输入后发送。")
                        .into_any_element()]
                } else {
                    messages
                        .into_iter()
                        .enumerate()
                        .map(|(i, (role, text))| {
                            let prefix = match role {
                                AiChatRole::User => "你",
                                AiChatRole::Assistant => "AI",
                            };
                            div()
                                .id(SharedString::from(format!("ai-msg-{i}")))
                                .text_sm()
                                .child(format!("{prefix}：{text}"))
                                .into_any_element()
                        })
                        .collect()
                }),
        )
        .child(
            v_flex()
                .id("ai-proposals")
                .gap_1()
                .children({
                    let mut rows: Vec<AnyElement> = vec![div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .child("提案")
                        .into_any_element()];
                    if proposals.is_empty() {
                        rows.push(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("暂无待确认提案")
                                .into_any_element(),
                        );
                    } else {
                        for (id, summary, _stale) in proposals {
                            rows.push(
                                h_flex()
                                    .id(SharedString::from(format!("ai-proposal-{id}")))
                                    .w_full()
                                    .gap_1()
                                    .items_center()
                                    .child(div().flex_1().text_xs().child(summary))
                                    .child(
                                        Button::new(format!("ai-apply-{id}"))
                                            .xsmall()
                                            .label("应用")
                                            .disabled(busy)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Err(err) =
                                                    this.state.ai_apply_proposal(id, Utc::now())
                                                {
                                                    this.state.ai.status_message =
                                                        Some(format!("{err:#}"));
                                                }
                                                cx.notify();
                                            })),
                                    )
                                    .child(
                                        Button::new(format!("ai-discard-{id}"))
                                            .ghost()
                                            .xsmall()
                                            .label("放弃")
                                            .disabled(busy)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Err(err) =
                                                    this.state.ai_discard_proposal(id)
                                                {
                                                    this.state.ai.status_message =
                                                        Some(format!("{err:#}"));
                                                }
                                                cx.notify();
                                            })),
                                    )
                                    .into_any_element(),
                            );
                        }
                    }
                    rows
                }),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("ai-apply-all")
                        .small()
                        .label("全部应用")
                        .disabled(!has_proposals || busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Err(err) = this.state.ai_apply_all_proposals(Utc::now()) {
                                this.state.ai.status_message = Some(format!("{err:#}"));
                            }
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("ai-discard-all")
                        .ghost()
                        .small()
                        .label("全部放弃")
                        .disabled(!has_proposals || busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.ai_discard_all_proposals();
                            cx.notify();
                        })),
                ),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div().flex_1().child(
                        ai_input
                            .map(|input| Input::new(&input).disabled(busy).into_any_element())
                            .unwrap_or_else(|| {
                                div()
                                    .p_2()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("输入框加载中…")
                                    .into_any_element()
                            }),
                    ),
                )
                .child(
                    Button::new("ai-send")
                        .small()
                        .label(if busy { "…" } else { "发送" })
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.send_ai_prompt(window, cx);
                        })),
                ),
        )
}
