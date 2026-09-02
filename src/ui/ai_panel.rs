use chrono::Utc;
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, button::Button,
    button::ButtonVariants as _, h_flex, input::Input, input::Textarea, menu::DropdownMenu as _,
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

    let auto_apply = workspace.state.ai.auto_apply();
    let busy = workspace.state.ai.busy;
    let max_token_tier = workspace.state.ai.max_token_tier();
    let agent = workspace.state.ai.agent();
    let proposals_expanded = workspace.state.ai.proposals_expanded;
    let status = workspace.state.ai.status_message.clone();
    let messages: Vec<(AiChatRole, String)> = workspace
        .state
        .ai
        .messages()
        .iter()
        .map(|m| (m.role, m.text.clone()))
        .collect();
    let proposals: Vec<(uuid::Uuid, String, bool)> = workspace
        .state
        .ai
        .proposals()
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
    let agent_label = agent.label();
    let context_refs = workspace.state.ai.context_refs.clone();
    let session_title = workspace.state.ai.active_title();
    let session_summaries = workspace.state.ai.list_summaries();
    let active_session_id = workspace.state.ai.active_id();
    let renaming_session = workspace.ai_renaming_session;

    v_flex()
        .id("ai-panel")
        .size_full()
        .bg(cx.theme().background)
        .border_l_1()
        .border_color(cx.theme().border)
        .child(render_session_header(
            workspace,
            session_title,
            session_summaries,
            active_session_id,
            renaming_session,
            busy,
            window,
            cx,
        ))
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
                    context_refs,
                    policy_label,
                    tier_label,
                    agent_label,
                    busy,
                    auto_apply,
                    max_token_tier,
                    agent,
                    cx,
                )),
        )
}

fn format_session_time(updated: chrono::DateTime<Utc>) -> String {
    updated.format("%m-%d %H:%M").to_string()
}

fn render_session_header(
    workspace: &mut Workspace,
    session_title: String,
    session_summaries: Vec<crate::ai::AiSessionSummary>,
    active_session_id: Option<uuid::Uuid>,
    renaming_session: bool,
    busy: bool,
    window: &mut Window,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    let workspace_entity = cx.entity();

    h_flex()
        .id("ai-session-header")
        .w_full()
        .items_center()
        .gap_2()
        .px_3()
        .pt_2()
        .pb_1()
        .child(
            Button::new("ai-new-session")
                .xsmall()
                .label("+ 新对话")
                .disabled(busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Err(err) = this.state.ai_new_session(Utc::now()) {
                        this.state.ai.status_message = Some(format!("{err:#}"));
                    }
                    this.cancel_ai_session_rename(window, cx);
                    cx.notify();
                })),
        )
        .child(if renaming_session {
            workspace.ensure_ai_session_title_input(window, cx);
            h_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .items_center()
                .child(
                    workspace
                        .ai_session_title_input
                        .clone()
                        .map(|input| {
                            Input::new(&input)
                                .small()
                                .w_full()
                                .into_any_element()
                        })
                        .unwrap_or_else(|| {
                            div()
                                .text_xs()
                                .child("…")
                                .into_any_element()
                        }),
                )
                .child(
                    Button::new("ai-rename-cancel")
                        .ghost()
                        .xsmall()
                        .label("取消")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_ai_session_rename(window, cx);
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            Button::new("ai-session-picker")
                .xsmall()
                .label(session_title.as_str())
                .disabled(busy)
                .dropdown_menu({
                    let workspace_entity = workspace_entity.clone();
                    move |menu, _, _| {
                        let mut menu = menu;
                        for summary in &session_summaries {
                            let ws = workspace_entity.clone();
                            let id = summary.id;
                            let mark = if active_session_id == Some(id) {
                                " ✓"
                            } else {
                                ""
                            };
                            let label = format!(
                                "{}{} · {}",
                                summary.title,
                                mark,
                                format_session_time(summary.updated_at)
                            );
                            menu = menu.item(
                                PopupMenuItem::new(label).on_click(move |_, _, cx| {
                                    ws.update(cx, |this, cx| {
                                        if let Err(err) =
                                            this.state.ai_switch_session(id, Utc::now())
                                        {
                                            this.state.ai.status_message =
                                                Some(format!("{err:#}"));
                                        }
                                        cx.notify();
                                    });
                                }),
                            );
                        }
                        let ws_rename = workspace_entity.clone();
                        let ws_delete = workspace_entity.clone();
                        menu = menu
                            .item(
                                PopupMenuItem::new("重命名…").on_click(move |_, window, cx| {
                                    ws_rename.update(cx, |this, cx| {
                                        this.begin_ai_session_rename(window, cx);
                                        cx.notify();
                                    });
                                }),
                            )
                            .item(
                                PopupMenuItem::new("删除当前对话").on_click(move |_, _, cx| {
                                    ws_delete.update(cx, |this, cx| {
                                        if let Some(id) = this.state.ai.active_id() {
                                            if let Err(err) =
                                                this.state.ai_delete_session(id, Utc::now())
                                            {
                                                this.state.ai.status_message =
                                                    Some(format!("{err:#}"));
                                            }
                                        }
                                        cx.notify();
                                    });
                                }),
                            );
                        menu
                    }
                })
                .into_any_element()
        })
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
    context_refs: Vec<crate::ai::AiContextRef>,
    policy_label: &'static str,
    tier_label: &'static str,
    agent_label: &'static str,
    busy: bool,
    auto_apply: bool,
    max_token_tier: AiMaxTokenTier,
    agent: crate::ai::AiAgentKind,
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
        .drag_over::<crate::ui::AiDragPayload>(|style, _, _, cx| {
            style.bg(cx.theme().drop_target)
        })
        .drag_over::<crate::ui::editor::DragBlock>(|style, _, _, cx| {
            style.bg(cx.theme().drop_target)
        })
        .drag_over::<crate::ui::editor::DragScriptBlock>(|style, _, _, cx| {
            style.bg(cx.theme().drop_target)
        })
        .on_drop(cx.listener(move |this, drag: &crate::ui::AiDragPayload, _, cx| {
            if this.state.ai.busy {
                return;
            }
            this.state.ai_add_context_ref(drag.context.clone());
            cx.notify();
        }))
        .on_drop(cx.listener(move |this, drag: &crate::ui::editor::DragBlock, _, cx| {
            if this.state.ai.busy {
                return;
            }
            this.state.ai_add_context_ref(drag.context.clone());
            cx.notify();
        }))
        .on_drop(cx.listener(
            move |this, drag: &crate::ui::editor::DragScriptBlock, _, cx| {
                if this.state.ai.busy {
                    return;
                }
                this.state.ai_add_context_ref(drag.context.clone());
                cx.notify();
            },
        ))
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
                .when(!context_refs.is_empty(), |wrap| {
                    wrap.child(render_context_chips(&context_refs, busy, cx))
                })
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
                    agent_label,
                    busy,
                    auto_apply,
                    max_token_tier,
                    agent,
                    cx,
                )),
        )
}

fn render_context_chips(
    refs: &[crate::ai::AiContextRef],
    busy: bool,
    cx: &mut Context<'_, Workspace>,
) -> impl IntoElement {
    h_flex()
        .id("ai-context-chips")
        .w_full()
        .gap_1()
        .flex_wrap()
        .children(refs.iter().enumerate().map(|(i, r)| {
            let label = format!("{} · {}", r.kind.label(), r.title);
            h_flex()
                .id(SharedString::from(format!("ai-chip-{i}")))
                .gap_1()
                .items_center()
                .px_1p5()
                .py_0p5()
                .rounded(cx.theme().radius)
                .bg(cx.theme().muted)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(label),
                )
                .child(
                    Button::new(format!("ai-chip-remove-{i}"))
                        .ghost()
                        .xsmall()
                        .label("×")
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.ai_remove_context_ref_at(i);
                            cx.notify();
                        })),
                )
                .into_any_element()
        }))
}

fn render_toolbar(
    policy_label: &'static str,
    tier_label: &'static str,
    agent_label: &'static str,
    busy: bool,
    auto_apply: bool,
    max_token_tier: AiMaxTokenTier,
    agent: crate::ai::AiAgentKind,
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
                                        this.state
                                            .set_ai_auto_apply(false, Utc::now());
                                        cx.notify();
                                    });
                                },
                            ),
                        )
                        .item(
                            PopupMenuItem::new(format!("始终允许{allow_mark}")).on_click(
                                move |_, _, cx| {
                                    ws_allow.update(cx, |this, cx| {
                                        this.state.set_ai_auto_apply(true, Utc::now());
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
                                            this.state
                                                .set_ai_max_token_tier(tier, Utc::now());
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
            Button::new("ai-agent")
                .xsmall()
                .label(agent_label)
                .disabled(busy)
                .dropdown_menu({
                    let workspace = workspace.clone();
                    move |menu, _, _| {
                        let mut menu = menu;
                        for kind in crate::ai::AiAgentKind::all() {
                            let workspace = workspace.clone();
                            let mark = if agent == kind { " ✓" } else { "" };
                            menu = menu.item(
                                PopupMenuItem::new(format!("{}{mark}", kind.label())).on_click(
                                    move |_, _, cx| {
                                        workspace.update(cx, |this, cx| {
                                            this.state.set_ai_agent(kind, Utc::now());
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
                                    this.state.ai_discard_all_proposals(Utc::now());
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
                                                    this.state.ai_discard_proposal(id, Utc::now())
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
