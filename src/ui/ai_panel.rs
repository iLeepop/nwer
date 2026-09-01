use chrono::Utc;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, StyledExt, button::Button,
    button::ButtonVariants as _, h_flex, input::Textarea, menu::DropdownMenu as _,
    menu::PopupMenuItem, v_flex,
};

use crate::ai::{summarize_intent, AiChatRole, AiMaxTokenTier};
use crate::ui::selectable_text::{selectable_markdown, selectable_plain};
use crate::ui::Workspace;

/// 提案列表可见高度约 3 行（行高 + 内边距）。
const PROPOSAL_LIST_MAX_H: f32 = 3.0 * 36.0 + 16.0;

/// AI 助手面板：消息色块、浮动提案、多行输入与底栏。
pub fn render_ai_panel(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    workspace.ensure_ai_input(window, cx);

    let auto_apply = workspace.state.ai.auto_apply;
    let busy = workspace.state.ai.busy;
    let max_token_tier = workspace.state.ai.max_token_tier;
    let proposals_expanded = workspace.state.ai.proposals_expanded;
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
    let proposal_count = proposals.len();
    let has_proposals = proposal_count > 0;
    let ai_input = workspace.ai_input.clone();
    let policy_label = if auto_apply {
        "始终允许"
    } else {
        "始终询问"
    };
    let tier_label = max_token_tier.label();

    v_flex()
        .id("ai-panel")
        .size_full()
        .bg(cx.theme().background)
        .border_l_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .w_full()
                .items_center()
                .px_3()
                .pt_3()
                .pb_1()
                .child(div().font_bold().child("AI 助手")),
        )
        .children(status.map(|msg| {
            div()
                .px_3()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(msg)
        }))
        .child(
            v_flex()
                .id("ai-chat-wrap")
                .flex_1()
                .min_h_0()
                .w_full()
                .child(
                    v_flex()
                        .id("ai-messages")
                        .flex_1()
                        .min_h_0()
                        .gap_2()
                        .px_3()
                        .py_2()
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
                                    render_message_block(i, role, text, cx)
                                })
                                .collect()
                        }),
                )
                .child(render_input_dock(
                    ai_input,
                    has_proposals,
                    proposal_count,
                    proposals_expanded,
                    proposals,
                    policy_label,
                    tier_label,
                    busy,
                    auto_apply,
                    max_token_tier,
                    cx,
                )),
        )
}

fn render_message_block(
    i: usize,
    role: AiChatRole,
    text: String,
    cx: &App,
) -> AnyElement {
    let (prefix, bg) = match role {
        AiChatRole::User => ("你", cx.theme().accent.opacity(0.18)),
        AiChatRole::Assistant => ("AI", cx.theme().muted),
    };
    let text_view = match role {
        AiChatRole::User => selectable_plain(format!("ai-msg-{i}"), text),
        AiChatRole::Assistant => selectable_markdown(format!("ai-msg-{i}"), text),
    };
    div()
        .id(SharedString::from(format!("ai-msg-wrap-{i}")))
        .w_full()
        .rounded_md()
        .px_3()
        .py_2()
        .bg(bg)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .mb_1()
                .child(prefix),
        )
        .child(text_view.text_sm().line_height(relative(1.6)))
        .into_any_element()
}

fn render_input_dock(
    ai_input: Option<Entity<gpui_component::input::TextareaState>>,
    has_proposals: bool,
    proposal_count: usize,
    proposals_expanded: bool,
    proposals: Vec<(uuid::Uuid, String, bool)>,
    policy_label: &'static str,
    tier_label: &'static str,
    busy: bool,
    auto_apply: bool,
    max_token_tier: AiMaxTokenTier,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    div()
        .id("ai-input-dock")
        .relative()
        .flex_shrink_0()
        .w_full()
        .border_t_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .px_3()
        .py_2()
        .when(has_proposals, |dock| {
            dock.child(render_proposals_float(
                proposal_count,
                proposals_expanded,
                proposals,
                busy,
                cx,
            ))
        })
        .child(
            v_flex()
                .id("ai-input-wrap")
                .w_full()
                .gap_2()
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .child(
                    ai_input
                        .map(|input| {
                            Textarea::new(&input)
                                .disabled(busy)
                                .appearance(false)
                                .bordered(false)
                                .into_any_element()
                        })
                        .unwrap_or_else(|| {
                            div()
                                .p_2()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("输入框加载中…")
                                .into_any_element()
                        }),
                )
                .child(render_toolbar(
                    policy_label,
                    tier_label,
                    busy,
                    auto_apply,
                    max_token_tier,
                    cx,
                )),
        )
}

fn render_toolbar(
    policy_label: &'static str,
    tier_label: &'static str,
    busy: bool,
    auto_apply: bool,
    max_token_tier: AiMaxTokenTier,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let workspace = cx.entity();
    h_flex()
        .w_full()
        .items_center()
        .gap_2()
        .child(
            Button::new("ai-write-policy")
                .xsmall()
                .label(policy_label)
                .disabled(busy)
                .dropdown_menu({
                    let workspace = workspace.clone();
                    move |menu, _, _| {
                        let ws_ask = workspace.clone();
                        let ws_allow = workspace.clone();
                        let ask_mark = if !auto_apply { " ✓" } else { "" };
                        let allow_mark = if auto_apply { " ✓" } else { "" };
                        menu.item(
                            PopupMenuItem::new(format!("始终询问{ask_mark}")).on_click(
                                move |_, _, cx| {
                                    ws_ask.update(cx, |this, cx| {
                                        this.state.set_ai_auto_apply(false);
                                        cx.notify();
                                    });
                                },
                            ),
                        )
                        .item(
                            PopupMenuItem::new(format!("始终允许{allow_mark}")).on_click(
                                move |_, _, cx| {
                                    ws_allow.update(cx, |this, cx| {
                                        this.state.set_ai_auto_apply(true);
                                        cx.notify();
                                    });
                                },
                            ),
                        )
                    }
                }),
        )
        .child(
            Button::new("ai-token-tier")
                .xsmall()
                .label(tier_label)
                .disabled(busy)
                .dropdown_menu({
                    let workspace = workspace.clone();
                    move |menu, _, _| {
                        let mut menu = menu;
                        for tier in AiMaxTokenTier::all() {
                            let workspace = workspace.clone();
                            let mark = if max_token_tier == tier { " ✓" } else { "" };
                            menu = menu.item(
                                PopupMenuItem::new(format!("{}{mark}", tier.label())).on_click(
                                    move |_, _, cx| {
                                        workspace.update(cx, |this, cx| {
                                            this.state.set_ai_max_token_tier(tier);
                                            cx.notify();
                                        });
                                    },
                                ),
                            );
                        }
                        menu
                    }
                }),
        )
        .child(div().flex_1())
        .child(
            Button::new("ai-send")
                .small()
                .label(if busy { "…" } else { "发送" })
                .disabled(busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.send_ai_prompt(window, cx);
                })),
        )
}

fn render_proposals_float(
    proposal_count: usize,
    proposals_expanded: bool,
    proposals: Vec<(uuid::Uuid, String, bool)>,
    busy: bool,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let toggle_label = if proposals_expanded { "▾" } else { "▴" };
    let toggle_title = if proposals_expanded {
        "折叠提案列表"
    } else {
        "展开提案列表"
    };

    div()
        .id("ai-proposals-float")
        .absolute()
        .bottom_full()
        .left_0()
        .right_0()
        .child(
            v_flex()
                .id("ai-proposals-card")
                .w(relative(0.92))
                .mx_auto()
                .rounded_t_md()
                .border_1()
                .border_b_0()
                .border_color(cx.theme().border)
                .bg(cx.theme().popover)
                .shadow_md()
                .child(
                    h_flex()
                        .id("ai-proposals-head")
                        .w_full()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().muted)
                        .child(
                            Button::new("ai-proposals-toggle")
                                .xsmall()
                                .label(toggle_label)
                                .tooltip(toggle_title)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.toggle_ai_proposals_expanded();
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("提案 · {proposal_count}")),
                        )
                        .child(
                            Button::new("ai-apply-all")
                                .xsmall()
                                .label("全部应用")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Err(err) = this.state.ai_apply_all_proposals(Utc::now()) {
                                        this.state.ai.status_message = Some(format!("{err:#}"));
                                    }
                                    this.reset_editor_after_ai_mutate();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("ai-discard-all")
                                .ghost()
                                .xsmall()
                                .label("全部放弃")
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.ai_discard_all_proposals();
                                    cx.notify();
                                })),
                        ),
                )
                .when(proposals_expanded, |card| {
                    card.child(
                        v_flex()
                            .id("ai-proposals-body")
                            .w_full()
                            .max_h(px(PROPOSAL_LIST_MAX_H))
                            .overflow_y_scroll()
                            .gap_0()
                            .px_1()
                            .py_1()
                            .children(proposals.into_iter().map(|(id, summary, _stale)| {
                                h_flex()
                                    .id(SharedString::from(format!("ai-proposal-{id}")))
                                    .w_full()
                                    .h(px(36.))
                                    .gap_1()
                                    .items_center()
                                    .px_1()
                                    .rounded_sm()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .overflow_x_hidden()
                                            .whitespace_nowrap()
                                            .child(summary),
                                    )
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
                                                this.reset_editor_after_ai_mutate();
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
                                    .into_any_element()
                            })),
                    )
                }),
        )
}
