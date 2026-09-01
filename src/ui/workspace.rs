use std::time::Duration;

use chrono::Utc;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, IndexPath, Root, Sizable as _, StyledExt, WindowExt as _,
    button::Button,
    dialog::DialogButtonProps,
    h_flex,
    input::Input,
    input::InputEvent,
    input::InputState,
    input::RopeExt as _,
    input::TextareaState,
    resizable::h_resizable,
    resizable::resizable_panel,
    select::{Select, SelectState},
    v_flex,
};

use crate::app::AppState;
use crate::app::SidebarTab;
use crate::models::{BlockFocus, BlockType, OutlineCategory, ScriptFocus};
use crate::storage::{
    ChapterNodeKind, MoveDirection, ScriptNodeKind, find_node_by_rel, find_script_node_by_rel,
};
use crate::ui::editor::{
    DeleteFocusedBlock, EscapeBlockFocus, FocusNextBlock, FocusPrevBlock, MergeSelectedBlocks,
    MoveFocusedBlockDown, MoveFocusedBlockUp, SaveDocument, SetTypeAside, SetTypeDialogue,
    SetTypeNarration, SetTypeNote, SetTypeSceneBreak, SetTypeThought,
};
use crate::ui::sidebar::chapter_tree::{
    CopyChapter, DeleteNode, MoveDown, MoveUp, NewChapter, NewDirectory, RenameNode,
};
use crate::ui::sidebar::script_tree::CopyScript;
use crate::ui::sidebar::outline_tree::{DeleteOutlineEntry, NewOutlineEntry, RenameOutlineEntry};
use crate::ui::{ai_panel, dialog_ok_cancel_footer, editor, sidebar, status_bar, top_bar};
use std::collections::HashMap;

pub struct Workspace {
    pub(crate) state: AppState,
    pub(crate) title_input: Option<Entity<InputState>>,
    pub(crate) editing_input: Option<(usize, Entity<TextareaState>)>,
    pub(crate) speaker_input: Option<(usize, Entity<InputState>)>,
    pub(crate) character_input: Option<(usize, Entity<InputState>)>,
    pub(crate) search_input: Option<Entity<InputState>>,
    pub(crate) ai_input: Option<Entity<InputState>>,
    pub(crate) outline_field_name_inputs: HashMap<String, Entity<InputState>>,
    pub(crate) outline_field_value_inputs: HashMap<String, Entity<TextareaState>>,
    _edit_subscriptions: Vec<Subscription>,
    _title_subscription: Option<Subscription>,
    _search_subscription: Option<Subscription>,
    _outline_subscriptions: Vec<Subscription>,
    debounce_gen: u64,
    _debounce_task: Option<Task<()>>,
    _ai_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    keys_bound: bool,
}

impl Workspace {
    pub fn new(state: AppState, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            title_input: None,
            editing_input: None,
            speaker_input: None,
            character_input: None,
            search_input: None,
            ai_input: None,
            outline_field_name_inputs: HashMap::new(),
            outline_field_value_inputs: HashMap::new(),
            _edit_subscriptions: Vec::new(),
            _title_subscription: None,
            _search_subscription: None,
            _outline_subscriptions: Vec::new(),
            debounce_gen: 0,
            _debounce_task: None,
            _ai_task: None,
            focus_handle: cx.focus_handle(),
            keys_bound: false,
        }
    }

    fn bind_keys_once(&mut self, cx: &mut Context<Self>) {
        if self.keys_bound {
            return;
        }
        self.keys_bound = true;
        cx.bind_keys([
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-s", SaveDocument, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-s", SaveDocument, None),
            KeyBinding::new("escape", EscapeBlockFocus, None),
            KeyBinding::new("ctrl-up", FocusPrevBlock, None),
            KeyBinding::new("ctrl-down", FocusNextBlock, None),
        ]);
    }

    pub(crate) fn invalidate_editor_inputs(&mut self) {
        self.editing_input = None;
        self.speaker_input = None;
        self._edit_subscriptions.clear();
    }

    pub(crate) fn invalidate_script_editor_inputs(&mut self) {
        self.editing_input = None;
        self.character_input = None;
        self._edit_subscriptions.clear();
    }

    pub(crate) fn invalidate_outline_inputs(&mut self) {
        self.outline_field_name_inputs.clear();
        self.outline_field_value_inputs.clear();
        self._outline_subscriptions.clear();
    }

    pub(crate) fn ensure_search_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_input.is_some() {
            return;
        }
        let query = self.state.search_query.clone();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("搜索……")
                .default_value(query)
        });
        // subscribe_in already provides &mut Workspace — do not nest workspace.update().
        self._search_subscription = Some(cx.subscribe_in(&input, window, {
            move |this, state, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    if let Err(err) = this.state.set_search_query(text) {
                        eprintln!("search failed: {err:#}");
                    }
                    cx.notify();
                }
            }
        }));
        self.search_input = Some(input);
    }

    pub(crate) fn ensure_ai_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai_input.is_some() {
            return;
        }
        self.ai_input = Some(cx.new(|cx| {
            InputState::new(window, cx).placeholder("向 AI 提问或下达写作指令…")
        }));
    }

    /// 从 AI 面板发送：校验配置 → hydrate → 后台跑 Host → 回写提案。
    pub(crate) fn send_ai_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.ai.busy {
            return;
        }
        self.ensure_ai_input(window, cx);
        let Some(input) = self.ai_input.clone() else {
            return;
        };
        let text = input.read(cx).value().to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }

        if let Err(err) = crate::ai::validate_ai_ready(self.state.ai_settings()) {
            self.state.ai_push_user_message(&text);
            self.state.ai_fail_run(err);
            input.update(cx, |s, cx| s.set_value("", window, cx));
            cx.notify();
            return;
        }

        let settings = self.state.ai_settings().clone();
        let lean = crate::ai::lean_context_from_app(&self.state);
        let shared = crate::ai::hydrate_shared_ctx(&self.state);
        let max_tokens = crate::ai::resolve_max_tokens(
            self.state.ai.max_token_tier,
            &settings.provider,
        );
        let max_tool_rounds =
            crate::storage::clamp_max_tool_rounds(settings.max_tool_rounds);

        self.state.ai_push_user_message(&text);
        self.state.ai_mark_busy(true);
        self.state.ai_begin_assistant_stream();
        input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();

        self._ai_task = Some(cx.spawn(async move |this, cx| {
            enum StreamEvent {
                Delta(String),
                Done(
                    anyhow::Result<(
                        String,
                        crate::ai::ProposalStore,
                        Vec<crate::ai::AiUiCommand>,
                    )>,
                ),
            }

            let (tx, rx) = std::sync::mpsc::channel::<StreamEvent>();
            let tx_done = tx.clone();
            let worker = std::thread::spawn(move || {
                let result = (|| {
                    let rt = tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| anyhow::anyhow!("tokio runtime: {e}"))?;
                    rt.block_on(async move {
                        let llm = crate::ai::build_llm(&settings)?;
                        let mut host = crate::ai::AiSessionHost::from_llm(llm, shared, lean);
                        let tx_delta = tx;
                        let reply = host
                            .run_stream(
                                crate::ai::AiAction::Chat,
                                &text,
                                max_tokens,
                                max_tool_rounds,
                                move |delta| {
                                    let _ = tx_delta.send(StreamEvent::Delta(delta.to_string()));
                                },
                            )
                            .await?;
                        let (proposals, ui_commands) = {
                            let mut guard = host
                                .ctx
                                .lock()
                                .map_err(|_| anyhow::anyhow!("ai ctx poisoned"))?;
                            let proposals = std::mem::take(&mut guard.proposals);
                            let ui_commands = guard.take_ui_commands();
                            (proposals, ui_commands)
                        };
                        anyhow::Ok((reply, proposals, ui_commands))
                    })
                })();
                let _ = tx_done.send(StreamEvent::Done(result));
            });

            loop {
                let mut finished = None;
                loop {
                    match rx.try_recv() {
                        Ok(StreamEvent::Delta(delta)) => {
                            this.update(cx, |this, cx| {
                                this.state.ai_append_stream_delta(&delta);
                                cx.notify();
                            })
                            .ok();
                        }
                        Ok(StreamEvent::Done(result)) => {
                            finished = Some(result);
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            finished =
                                Some(Err(anyhow::anyhow!("AI stream channel disconnected")));
                            break;
                        }
                    }
                }

                if let Some(run_result) = finished {
                    let _ = worker.join();
                    this.update(cx, |this, cx| {
                        match run_result {
                            Ok((reply, proposals, ui_commands)) => {
                                if let Err(err) = this.state.ai_ingest_host_result(
                                    reply,
                                    proposals,
                                    ui_commands,
                                    Utc::now(),
                                ) {
                                    this.state.ai_fail_run(err);
                                }
                            }
                            Err(err) => this.state.ai_fail_run(err),
                        }
                        cx.notify();
                    })
                    .ok();
                    break;
                }

                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
        }));
    }

    pub(crate) fn ensure_outline_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.state.current_outline.clone() else {
            self.invalidate_outline_inputs();
            return;
        };
        let keys: Vec<String> = entry.fields.keys().cloned().collect();
        // 移除已删除字段的输入
        self.outline_field_name_inputs
            .retain(|k, _| keys.contains(k));
        self.outline_field_value_inputs
            .retain(|k, _| keys.contains(k));

        for key in keys {
            if self.outline_field_name_inputs.contains_key(&key) {
                continue;
            }
            let value = entry.fields.get(&key).cloned().unwrap_or_default();
            let name_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("字段名")
                    .default_value(key.clone())
            });
            let value_input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(1, 8)
                    .placeholder("字段值")
                    .default_value(value)
            });

            let field_key = key.clone();
            self._outline_subscriptions
                .push(cx.subscribe_in(&name_input, window, {
                    let old_key = field_key.clone();
                    move |this, state, event, window, cx| {
                        if matches!(event, InputEvent::Blur) {
                            let new_key = state.read(cx).value().to_string();
                            let new_key = new_key.trim().to_string();
                            if new_key.is_empty() || new_key == old_key {
                                return;
                            }
                            if let Err(err) =
                                this.state
                                    .rename_outline_field(&old_key, &new_key, Utc::now())
                            {
                                eprintln!("rename field failed: {err:#}");
                                if let Some(input) = this.outline_field_name_inputs.get(&old_key) {
                                    input.update(cx, |s, cx| {
                                        s.set_value(old_key.clone(), window, cx);
                                    });
                                }
                            } else {
                                this.invalidate_outline_inputs();
                                this.ensure_outline_form(window, cx);
                            }
                            cx.notify();
                        }
                    }
                }));

            let field_key = key.clone();
            self._outline_subscriptions
                .push(cx.subscribe_in(&value_input, window, {
                    move |this, state, event, _, cx| {
                        if matches!(event, InputEvent::Change) {
                            let text = state.read(cx).value().to_string();
                            let _ = this.state.set_outline_field(
                                &field_key,
                                text,
                                Utc::now(),
                                monotonic_ms(),
                            );
                            this.schedule_debounced_save(cx);
                            cx.notify();
                        }
                    }
                }));

            self.outline_field_name_inputs
                .insert(key.clone(), name_input);
            self.outline_field_value_inputs.insert(key, value_input);
        }
    }

    pub(crate) fn prompt_new_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.project_dir.is_none() {
            return;
        }
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("条目名称")
                .default_value("未命名")
        });
        let category_labels: Vec<SharedString> = OutlineCategory::all()
            .into_iter()
            .map(|c| SharedString::from(c.label()))
            .collect();
        let category_select =
            cx.new(|cx| SelectState::new(category_labels, Some(IndexPath::default()), window, cx));
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let name_input = name_input.clone();
            let category_select = category_select.clone();
            let workspace = workspace.clone();
            dialog
                .title("新建大纲条目")
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().child("名称"))
                        .child(Input::new(&name_input))
                        .child(div().text_sm().child("分类"))
                        .child(Select::new(&category_select).placeholder("选择分类")),
                )
                .footer(dialog_ok_cancel_footer("创建", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("创建")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = name_input.read(cx).value().to_string();
                            let category = category_select
                                .read(cx)
                                .selected_value()
                                .map(|s| parse_outline_category(s.as_ref()))
                                .unwrap_or(OutlineCategory::Character);
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state
                                    .create_outline(name.trim(), category, Utc::now())?;
                                this.invalidate_editor_inputs();
                                this.title_input = None;
                                this.invalidate_outline_inputs();
                                this.ensure_outline_form(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("create outline failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_rename_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.state.current_outline.clone() else {
            return;
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("新名称")
                .default_value(entry.key.clone())
        });
        let workspace = cx.entity();
        let id = entry.id;
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            dialog
                .title("重命名大纲条目")
                .child(Input::new(&input))
                .footer(dialog_ok_cancel_footer("确定", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("确定")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.rename_outline(id, name.trim(), Utc::now())?;
                                this.invalidate_outline_inputs();
                                this.ensure_outline_form(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("rename outline failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn confirm_delete_outline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.state.current_outline.clone() else {
            return;
        };
        let workspace = cx.entity();
        let id = entry.id;
        let key = entry.key.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let workspace = workspace.clone();
            alert
                .title("确认删除大纲条目")
                .description(format!("确定删除「{key}」？"))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    if let Err(err) = workspace.update(cx, |this, cx| {
                        this.state.delete_outline(id, Utc::now())?;
                        this.invalidate_outline_inputs();
                        cx.notify();
                        anyhow::Ok(())
                    }) {
                        eprintln!("delete outline failed: {err:#}");
                    }
                    true
                })
        });
    }

    pub(crate) fn prompt_add_outline_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.current_outline.is_none() {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("字段名")
                .default_value("新字段")
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            dialog
                .title("添加字段")
                .child(Input::new(&input))
                .footer(dialog_ok_cancel_footer("添加", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("添加")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.add_outline_field(name.trim(), Utc::now())?;
                                this.invalidate_outline_inputs();
                                this.ensure_outline_form(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("add field failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("项目名称")
                .default_value(format!("新项目-{}", Utc::now().format("%Y%m%d-%H%M%S")))
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            dialog
                .title("新建项目")
                .child(Input::new(&input))
                .footer(dialog_ok_cancel_footer("创建", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("创建")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, _, cx| {
                            let title = input.read(cx).value().to_string();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.new_project(title.trim(), Utc::now())?;
                                this.title_input = None;
                                this.invalidate_editor_inputs();
                                this.invalidate_outline_inputs();
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("new project failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let recent_hint = self
            .state
            .config
            .recent_projects
            .iter()
            .take(5)
            .map(|r| format!("· {} — {}", r.title, r.path))
            .collect::<Vec<_>>()
            .join("\n");
        let default = self
            .state
            .config
            .recent_projects
            .first()
            .map(|r| r.path.clone())
            .unwrap_or_else(|| {
                self.state
                    .expanded_projects_root()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("项目文件夹路径")
                .default_value(default)
        });
        let workspace = cx.entity();
        let description = if recent_hint.is_empty() {
            "输入项目文件夹绝对路径（第一版无原生文件夹选择器）".to_string()
        } else {
            format!("最近项目：\n{recent_hint}\n\n或输入路径：")
        };
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            let description = description.clone();
            dialog
                .title("打开项目")
                .child(
                    v_flex()
                        .gap_2()
                        .child(div().text_sm().child(description))
                        .child(Input::new(&input)),
                )
                .footer(dialog_ok_cancel_footer("打开", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("打开")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, _, cx| {
                            let path = input.read(cx).value().to_string();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                let expanded = crate::app::expand_user_path(path.trim())?;
                                this.state.open_project(expanded, Utc::now())?;
                                this.title_input = None;
                                this.invalidate_editor_inputs();
                                this.invalidate_outline_inputs();
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("open project failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 已在 Workspace update 中：勿再 workspace.read(cx)。
        let root = self.state.config.projects_root.clone();
        let depth = self.state.max_depth().to_string();
        let ai = self.state.ai_settings().clone();
        let has_project = self.state.project.is_some();
        let workspace = cx.entity();
        crate::ui::settings::open_settings_dialog(
            workspace,
            root,
            depth,
            ai,
            has_project,
            window,
            cx,
        );
    }

    pub(crate) fn open_recent_at(&mut self, index: usize) -> anyhow::Result<()> {
        let recent = self
            .state
            .config
            .recent_projects
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("recent index out of range"))?;
        let path = crate::app::expand_user_path(&recent.path)?;
        self.state.open_project(path, Utc::now())?;
        self.title_input = None;
        self.invalidate_editor_inputs();
        self.invalidate_outline_inputs();
        Ok(())
    }

    pub(crate) fn quit_app(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = self.state.save_before_quit(Utc::now()) {
            eprintln!("save before quit failed: {err:#}");
        }
        cx.quit();
    }

    pub(crate) fn save_document(&mut self, cx: &mut Context<Self>) {
        if let Err(err) = self.state.save_manual(Utc::now()) {
            eprintln!("save failed: {err:#}");
        }
        cx.notify();
    }

    pub(crate) fn confirm_delete_block(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.block_focus.selected_index() else {
            return;
        };
        let workspace = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let workspace = workspace.clone();
            alert
                .title("确认删除段落块")
                .description(format!("确定删除第 {} 个段落块？", index + 1))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    if let Err(err) = workspace.update(cx, |this, cx| {
                        this.state.delete_block_at(index, Utc::now())?;
                        this.invalidate_editor_inputs();
                        cx.notify();
                        anyhow::Ok(())
                    }) {
                        eprintln!("delete block failed: {err:#}");
                    }
                    true
                })
        });
    }

    pub(crate) fn ensure_editing_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let BlockFocus::Editing { index } = self.state.block_focus else {
            return;
        };
        if self
            .editing_input
            .as_ref()
            .is_some_and(|(i, _)| *i == index)
        {
            return;
        }

        let Some(chapter) = self.state.current_chapter.as_ref() else {
            return;
        };
        let Some(block) = chapter.blocks.get(index) else {
            return;
        };
        let content = block.content.clone();
        let is_speaker_block = block.block_type.allows_speaker();
        let speaker = block.speaker.clone().unwrap_or_default();

        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 20)
                .placeholder("输入正文…")
                .default_value(content)
                .submit_on_enter(true)
        });

        let mut subs = Vec::new();
        // subscribe_in already provides &mut Workspace — do not nest workspace.update().
        subs.push(cx.subscribe_in(&input, window, {
            move |this, state, event, window, cx| match event {
                InputEvent::Change => {
                    let text = state.read(cx).value().to_string();
                    let index = match this.state.block_focus {
                        BlockFocus::Editing { index } => index,
                        _ => return,
                    };
                    let _ =
                        this.state
                            .set_block_content_at(index, text, Utc::now(), monotonic_ms());
                    this.schedule_debounced_save(cx);
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    if *shift {
                        return;
                    }
                    let cursor = state.read(cx).cursor();
                    let index = match this.state.block_focus {
                        BlockFocus::Editing { index } => index,
                        _ => return,
                    };
                    let text = this
                        .editing_input
                        .as_ref()
                        .map(|(_, e)| e.read(cx).value().to_string())
                        .unwrap_or_default();
                    let _ =
                        this.state
                            .set_block_content_at(index, text, Utc::now(), monotonic_ms());
                    if let Err(err) = this.state.split_block_at_cursor(index, cursor, Utc::now()) {
                        eprintln!("split failed: {err:#}");
                    }
                    this.invalidate_editor_inputs();
                    this.ensure_editing_input(window, cx);
                    cx.notify();
                }
                InputEvent::Blur => {}
                _ => {}
            }
        }));

        if is_speaker_block {
            let placeholder = if block.block_type == BlockType::Thought {
                "人物"
            } else {
                "说话人"
            };
            let speaker_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(speaker)
            });
            subs.push(cx.subscribe_in(&speaker_input, window, {
                move |this, state, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let text = state.read(cx).value().to_string();
                        let index = match this.state.block_focus {
                            BlockFocus::Editing { index } => index,
                            _ => return,
                        };
                        let speaker = {
                            let t = text.trim();
                            if t.is_empty() {
                                None
                            } else {
                                Some(t.to_string())
                            }
                        };
                        let _ = this.state.set_block_speaker_at(index, speaker, Utc::now());
                        this.schedule_debounced_save(cx);
                        cx.notify();
                    }
                }
            }));
            self.speaker_input = Some((index, speaker_input));
        } else {
            self.speaker_input = None;
        }

        self.editing_input = Some((index, input.clone()));
        self._edit_subscriptions = subs;

        input.update(cx, |state, cx| {
            let end = state.text().len();
            let position = state.text().offset_to_position(end);
            state.set_cursor_position(position, window, cx);
        });
    }

    pub(crate) fn ensure_script_editing_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ScriptFocus::Editing { index } = self.state.script_focus else {
            return;
        };
        if self
            .editing_input
            .as_ref()
            .is_some_and(|(i, _)| *i == index)
        {
            return;
        }

        let Some(script) = self.state.current_script.as_ref() else {
            return;
        };
        let Some(block) = script.blocks.get(index) else {
            return;
        };
        let content = block.content.clone();
        let allows_character = block.block_type.allows_character();
        let character = block.character.clone().unwrap_or_default();

        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 20)
                .placeholder("输入正文…")
                .default_value(content)
                .submit_on_enter(true)
        });

        let mut subs = Vec::new();
        subs.push(cx.subscribe_in(&input, window, {
            move |this, state, event, window, cx| match event {
                InputEvent::Change => {
                    let text = state.read(cx).value().to_string();
                    let index = match this.state.script_focus {
                        ScriptFocus::Editing { index } => index,
                        _ => return,
                    };
                    let _ = this.state.set_script_block_content_at(
                        index,
                        text,
                        Utc::now(),
                        monotonic_ms(),
                    );
                    this.schedule_debounced_save(cx);
                    cx.notify();
                }
                InputEvent::PressEnter { shift, .. } => {
                    if *shift {
                        return;
                    }
                    let cursor = state.read(cx).cursor();
                    let index = match this.state.script_focus {
                        ScriptFocus::Editing { index } => index,
                        _ => return,
                    };
                    let text = this
                        .editing_input
                        .as_ref()
                        .map(|(_, e)| e.read(cx).value().to_string())
                        .unwrap_or_default();
                    let _ = this.state.set_script_block_content_at(
                        index,
                        text,
                        Utc::now(),
                        monotonic_ms(),
                    );
                    if let Err(err) =
                        this.state.split_script_block_at_cursor(index, cursor, Utc::now())
                    {
                        eprintln!("split script block failed: {err:#}");
                    }
                    this.invalidate_script_editor_inputs();
                    this.ensure_script_editing_input(window, cx);
                    cx.notify();
                }
                InputEvent::Blur => {}
                _ => {}
            }
        }));

        if allows_character {
            let character_input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("角色")
                    .default_value(character)
            });
            subs.push(cx.subscribe_in(&character_input, window, {
                move |this, state, event, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        let text = state.read(cx).value().to_string();
                        let index = match this.state.script_focus {
                            ScriptFocus::Editing { index } => index,
                            _ => return,
                        };
                        let character = {
                            let t = text.trim();
                            if t.is_empty() {
                                None
                            } else {
                                Some(t.to_string())
                            }
                        };
                        let _ = this
                            .state
                            .set_script_character_at(index, character, Utc::now());
                        this.schedule_debounced_save(cx);
                        cx.notify();
                    }
                }
            }));
            self.character_input = Some((index, character_input));
        } else {
            self.character_input = None;
        }

        self.editing_input = Some((index, input.clone()));
        self._edit_subscriptions = subs;

        input.update(cx, |state, cx| {
            let end = state.text().len();
            let position = state.text().offset_to_position(end);
            state.set_cursor_position(position, window, cx);
        });
    }

    pub(crate) fn confirm_delete_script_block(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.script_focus.selected_index() else {
            return;
        };
        let workspace = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let workspace = workspace.clone();
            alert
                .title("确认删除剧本块")
                .description(format!("确定删除第 {} 个剧本块？", index + 1))
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    if let Err(err) = workspace.update(cx, |this, cx| {
                        this.state.delete_script_block_at(index, Utc::now())?;
                        this.invalidate_script_editor_inputs();
                        cx.notify();
                        anyhow::Ok(())
                    }) {
                        eprintln!("delete script block failed: {err:#}");
                    }
                    true
                })
        });
    }

    fn schedule_debounced_save(&mut self, cx: &mut Context<Self>) {
        self.debounce_gen = self.debounce_gen.wrapping_add(1);
        let generation = self.debounce_gen;
        self._debounce_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            this.update(cx, |this, cx| {
                if this.debounce_gen == generation {
                    if let Err(err) = this.state.save_now() {
                        eprintln!("autosave failed: {err:#}");
                    }
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    fn set_focused_block_type(&mut self, block_type: BlockType, cx: &mut Context<Self>) {
        let Some(index) = self.state.block_focus.selected_index() else {
            return;
        };
        if let Err(err) = self.state.set_block_type_at(index, block_type, Utc::now()) {
            eprintln!("set block type failed: {err:#}");
        }
        self.invalidate_editor_inputs();
        if matches!(self.state.block_focus, BlockFocus::Editing { .. }) {
            // 保持编辑态时重建输入
        }
        cx.notify();
    }

    /// 新建目录的父路径：选中目录则用该目录，选中叶子则用其父，否则根。
    fn creation_parent(&self) -> String {
        match self.state.ui.sidebar_tab {
            SidebarTab::Scripts => match self.state.ui.selected_node.as_deref() {
                Some(rel) => {
                    if let Some(node) = find_script_node_by_rel(&self.state.script_tree, rel)
                        && node.kind == ScriptNodeKind::Directory
                    {
                        return rel.to_string();
                    }
                    parent_of(rel).to_string()
                }
                None => String::new(),
            },
            _ => match self.state.ui.selected_node.as_deref() {
                Some(rel) => {
                    if let Some(node) = find_node_by_rel(&self.state.chapter_tree, rel)
                        && node.kind == ChapterNodeKind::Directory
                    {
                        return rel.to_string();
                    }
                    parent_of(rel).to_string()
                }
                None => String::new(),
            },
        }
    }

    fn unique_child_name(&self, parent_rel: &str, prefix: &str) -> String {
        match self.state.ui.sidebar_tab {
            SidebarTab::Scripts => {
                let siblings: &[crate::storage::ScriptTreeNode] = if parent_rel.is_empty() {
                    self.state.script_tree.as_slice()
                } else {
                    find_script_node_by_rel(&self.state.script_tree, parent_rel)
                        .map(|n| n.children.as_slice())
                        .unwrap_or(&[])
                };
                Self::unique_name_in(siblings, prefix)
            }
            _ => {
                let siblings: &[crate::storage::ChapterTreeNode] = if parent_rel.is_empty() {
                    self.state.chapter_tree.as_slice()
                } else {
                    find_node_by_rel(&self.state.chapter_tree, parent_rel)
                        .map(|n| n.children.as_slice())
                        .unwrap_or(&[])
                };
                Self::unique_chapter_name_in(siblings, prefix)
            }
        }
    }

    fn unique_name_in(siblings: &[crate::storage::ScriptTreeNode], prefix: &str) -> String {
        for i in 1..1000 {
            let candidate = format!("{prefix}{i:03}");
            let taken = siblings.iter().any(|c| c.name == candidate);
            if !taken {
                return candidate;
            }
        }
        format!("{prefix}{}", Utc::now().timestamp())
    }

    fn unique_chapter_name_in(siblings: &[crate::storage::ChapterTreeNode], prefix: &str) -> String {
        for i in 1..1000 {
            let candidate = format!("{prefix}{i:03}");
            let taken = siblings.iter().any(|c| c.name == candidate);
            if !taken {
                return candidate;
            }
        }
        format!("{prefix}{}", Utc::now().timestamp())
    }

    pub(crate) fn prompt_new_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.project_dir.is_none() {
            return;
        }
        let parent = self.creation_parent();
        let default = self.unique_child_name(&parent, "dir-");
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("目录名")
                .default_value(default)
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            dialog
                .title("新建目录")
                .child(Input::new(&input))
                .footer(dialog_ok_cancel_footer("创建", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("创建")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            let name = name.trim().to_string();
                            let parent = workspace.read(cx).creation_parent();
                            let new_rel = if parent.is_empty() {
                                name.clone()
                            } else {
                                format!("{parent}/{name}")
                            };
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.create_dir_under(&parent, &name, Utc::now())?;
                                this.state.select_directory(&new_rel);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("create directory failed: {err:#}");
                                return false;
                            }
                            let _ = window;
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_new_chapter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.project_dir.is_none() {
            return;
        }
        let parent = self.creation_parent();
        let default = self.unique_child_name(&parent, "ch-");
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("章节文件名（不含 .json）")
                .default_value(default)
        });
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("章节标题")
                .default_value("未命名章节")
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let name_input = name_input.clone();
            let title_input = title_input.clone();
            let workspace = workspace.clone();
            dialog
                .title("新建章节")
                .child(
                    v_flex()
                        .gap_2()
                        .child(Input::new(&name_input))
                        .child(Input::new(&title_input)),
                )
                .footer(dialog_ok_cancel_footer("创建", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("创建")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = name_input.read(cx).value().to_string();
                            let title = title_input.read(cx).value().to_string();
                            let parent = workspace.read(cx).creation_parent();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.create_chapter_under(
                                    &parent,
                                    name.trim(),
                                    title.trim(),
                                    Utc::now(),
                                )?;
                                this.invalidate_editor_inputs();
                                this.sync_title_input(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("create chapter failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_new_script(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.project_dir.is_none() {
            return;
        }
        let parent = self.creation_parent();
        let default = self.unique_child_name(&parent, "sc-");
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("剧本文件名（不含 .json）")
                .default_value(default)
        });
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("剧本标题")
                .default_value("未命名剧本")
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let name_input = name_input.clone();
            let title_input = title_input.clone();
            let workspace = workspace.clone();
            dialog
                .title("新建剧本")
                .child(
                    v_flex()
                        .gap_2()
                        .child(Input::new(&name_input))
                        .child(Input::new(&title_input)),
                )
                .footer(dialog_ok_cancel_footer("创建", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("创建")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = name_input.read(cx).value().to_string();
                            let title = title_input.read(cx).value().to_string();
                            let parent = workspace.read(cx).creation_parent();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.create_script_under(
                                    &parent,
                                    name.trim(),
                                    title.trim(),
                                    Utc::now(),
                                )?;
                                this.invalidate_script_editor_inputs();
                                this.sync_title_input(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("create script failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn prompt_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rel) = self.state.ui.selected_node.clone() else {
            return;
        };
        let current_name = match self.state.ui.sidebar_tab {
            SidebarTab::Scripts => find_script_node_by_rel(&self.state.script_tree, &rel)
                .map(|n| n.name.clone())
                .unwrap_or_default(),
            _ => find_node_by_rel(&self.state.chapter_tree, &rel)
                .map(|n| n.name.clone())
                .unwrap_or_default(),
        };
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("新名称")
                .default_value(current_name)
        });
        let workspace = cx.entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let workspace = workspace.clone();
            let rel = rel.clone();
            dialog
                .title("重命名")
                .child(Input::new(&input))
                .footer(dialog_ok_cancel_footer("确定", "取消"))
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("确定")
                        .show_cancel(true)
                        .cancel_text("取消")
                        .on_ok(move |_, window, cx| {
                            let name = input.read(cx).value().to_string();
                            if let Err(err) = workspace.update(cx, |this, cx| {
                                this.state.rename_at(&rel, name.trim(), Utc::now())?;
                                this.sync_title_input(window, cx);
                                cx.notify();
                                anyhow::Ok(())
                            }) {
                                eprintln!("rename failed: {err:#}");
                                return false;
                            }
                            true
                        }),
                )
        });
    }

    pub(crate) fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rel) = self.state.ui.selected_node.clone() else {
            return;
        };
        let is_dir = match self.state.ui.sidebar_tab {
            SidebarTab::Scripts => find_script_node_by_rel(&self.state.script_tree, &rel)
                .map(|n| n.kind == ScriptNodeKind::Directory)
                .unwrap_or(false),
            _ => find_node_by_rel(&self.state.chapter_tree, &rel)
                .map(|n| n.kind == ChapterNodeKind::Directory)
                .unwrap_or(false),
        };
        let nonempty = is_dir && self.state.directory_is_nonempty(&rel).unwrap_or(false);
        let description = if nonempty {
            format!("「{rel}」及其所有后代将被永久删除，此操作不可撤销。")
        } else {
            format!("确定删除「{rel}」？")
        };
        let workspace = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let workspace = workspace.clone();
            let rel = rel.clone();
            let description = description.clone();
            alert
                .title("确认删除")
                .description(description)
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    if let Err(err) = workspace.update(cx, |this, cx| {
                        this.state.delete_at(&rel, Utc::now())?;
                        this.title_input = None;
                        this.invalidate_editor_inputs();
                        this.invalidate_script_editor_inputs();
                        cx.notify();
                        anyhow::Ok(())
                    }) {
                        eprintln!("delete failed: {err:#}");
                    }
                    true
                })
        });
    }

    pub(crate) fn copy_selected_chapter(&mut self) {
        let Some(rel) = self.state.ui.selected_node.clone() else {
            return;
        };
        let Some(node) = find_node_by_rel(&self.state.chapter_tree, &rel).cloned() else {
            return;
        };
        if node.kind != ChapterNodeKind::Chapter {
            return;
        }
        let parent = parent_of(&rel).to_string();
        let new_name = format!("{}-副本", node.name);
        if let Err(err) = self
            .state
            .copy_chapter_at(&rel, &parent, &new_name, Utc::now())
        {
            eprintln!("copy chapter failed: {err:#}");
        }
    }

    pub(crate) fn copy_selected_script(&mut self) {
        let Some(rel) = self.state.ui.selected_node.clone() else {
            return;
        };
        let Some(node) = find_script_node_by_rel(&self.state.script_tree, &rel).cloned() else {
            return;
        };
        if node.kind != ScriptNodeKind::Script {
            return;
        }
        let parent = parent_of(&rel).to_string();
        let new_name = format!("{}-副本", node.name);
        if let Err(err) = self
            .state
            .copy_script_at(&rel, &parent, &new_name, Utc::now())
        {
            eprintln!("copy script failed: {err:#}");
        }
    }

    fn sync_title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(script) = self.state.current_script.as_ref() {
            let title = script.title.clone();
            match &self.title_input {
                Some(input) => {
                    input.update(cx, |state, cx| {
                        state.set_value(title, window, cx);
                    });
                }
                None => {
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("剧本标题")
                            .default_value(title)
                    });
                    self._title_subscription = Some(cx.subscribe_in(&input, window, {
                        move |this, state, event, _, cx| {
                            if matches!(event, InputEvent::Change) {
                                let title = state.read(cx).value().to_string();
                                if let Some(script) = this.state.current_script.as_mut() {
                                    script.title = title;
                                    this.state.dirty = true;
                                    this.schedule_debounced_save(cx);
                                }
                                cx.notify();
                            }
                        }
                    }));
                    self.title_input = Some(input);
                }
            }
            return;
        }
        if let Some(chapter) = self.state.current_chapter.as_ref() {
            let title = chapter.title.clone();
            match &self.title_input {
                Some(input) => {
                    input.update(cx, |state, cx| {
                        state.set_value(title, window, cx);
                    });
                }
                None => {
                    let input = cx.new(|cx| {
                        InputState::new(window, cx)
                            .placeholder("章节标题")
                            .default_value(title)
                    });
                    self._title_subscription = Some(cx.subscribe_in(&input, window, {
                        move |this, state, event, _, cx| {
                            if matches!(event, InputEvent::Change) {
                                let title = state.read(cx).value().to_string();
                                if let Some(ch) = this.state.current_chapter.as_mut() {
                                    ch.title = title;
                                    this.state.dirty = true;
                                    this.schedule_debounced_save(cx);
                                }
                                cx.notify();
                            }
                        }
                    }));
                    self.title_input = Some(input);
                }
            }
        } else {
            self.title_input = None;
        }
    }

    pub(crate) fn ensure_title_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.current_script.is_some() && self.title_input.is_none() {
            self.sync_title_input(window, cx);
        }
        if self.state.current_chapter.is_some() && self.title_input.is_none() {
            self.sync_title_input(window, cx);
        }
        if self.state.current_chapter.is_none() && self.state.current_script.is_none() {
            self.title_input = None;
        }
    }
}

fn parent_of(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(i) => &rel[..i],
        None => "",
    }
}

fn monotonic_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_outline_category(label: &str) -> OutlineCategory {
    match label {
        "背景" => OutlineCategory::Background,
        "场景" => OutlineCategory::Scene,
        "事件" => OutlineCategory::Event,
        "杂项" => OutlineCategory::Misc,
        _ => OutlineCategory::Character,
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.bind_keys_once(cx);
        self.ensure_title_input(window, cx);
        if matches!(self.state.block_focus, BlockFocus::Editing { .. }) {
            self.ensure_editing_input(window, cx);
        }
        if matches!(self.state.script_focus, ScriptFocus::Editing { .. }) {
            self.ensure_script_editing_input(window, cx);
        }

        let editor_title = self
            .state
            .current_script
            .as_ref()
            .map(|s| s.title.clone())
            .or_else(|| self.state.current_chapter.as_ref().map(|c| c.title.clone()))
            .unwrap_or_else(|| {
                if self.state.project.is_some() {
                    if self.state.ui.sidebar_tab == SidebarTab::Scripts {
                        "选择一个剧本".to_string()
                    } else {
                        "选择一个章节".to_string()
                    }
                } else {
                    "当前标题（未打开项目）".to_string()
                }
            });

        // Dialog / notification / sheet overlays live on Root; the first-level
        // app view must paint these layers or open_dialog / open_alert_dialog
        // appear to do nothing.
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);
        let sheet_layer = Root::render_sheet_layer(window, cx);

        div()
            .id("workspace-root")
            .size_full()
            .relative()
            .child(
                v_flex()
            .id("workspace")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .key_context("Workspace")
            .on_action(cx.listener(|this, _: &NewDirectory, window, cx| {
                this.prompt_new_directory(window, cx);
            }))
            .on_action(cx.listener(|this, _: &NewChapter, window, cx| {
                this.prompt_new_chapter(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RenameNode, window, cx| {
                this.prompt_rename(window, cx);
            }))
            .on_action(cx.listener(|this, _: &DeleteNode, window, cx| {
                this.confirm_delete(window, cx);
            }))
            .on_action(cx.listener(|this, _: &CopyChapter, _, cx| {
                this.copy_selected_chapter();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &CopyScript, _, cx| {
                this.copy_selected_script();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| {
                if let Err(err) = this
                    .state
                    .move_selected_sibling(MoveDirection::Up, Utc::now())
                {
                    eprintln!("move up failed: {err:#}");
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| {
                if let Err(err) = this
                    .state
                    .move_selected_sibling(MoveDirection::Down, Utc::now())
                {
                    eprintln!("move down failed: {err:#}");
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SaveDocument, _, cx| {
                this.save_document(cx);
            }))
            .on_action(cx.listener(|this, _: &EscapeBlockFocus, _, cx| {
                if this.state.current_script.is_some() {
                    this.state.escape_script_focus();
                    if !this.state.script_focus.is_editing() {
                        this.invalidate_script_editor_inputs();
                    }
                } else {
                    this.state.escape_block_focus();
                    if !this.state.block_focus.is_editing() {
                        this.invalidate_editor_inputs();
                    }
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusPrevBlock, window, cx| {
                if this.state.current_script.is_some() {
                    this.state.move_script_block_focus(-1);
                    this.invalidate_script_editor_inputs();
                    if matches!(this.state.script_focus, ScriptFocus::Editing { .. }) {
                        this.ensure_script_editing_input(window, cx);
                    }
                } else {
                    this.state.move_block_focus(-1);
                    this.invalidate_editor_inputs();
                    if matches!(this.state.block_focus, BlockFocus::Editing { .. }) {
                        this.ensure_editing_input(window, cx);
                    }
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusNextBlock, window, cx| {
                if this.state.current_script.is_some() {
                    this.state.move_script_block_focus(1);
                    this.invalidate_script_editor_inputs();
                    if matches!(this.state.script_focus, ScriptFocus::Editing { .. }) {
                        this.ensure_script_editing_input(window, cx);
                    }
                } else {
                    this.state.move_block_focus(1);
                    this.invalidate_editor_inputs();
                    if matches!(this.state.block_focus, BlockFocus::Editing { .. }) {
                        this.ensure_editing_input(window, cx);
                    }
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &DeleteFocusedBlock, window, cx| {
                if this.state.current_script.is_some() {
                    this.confirm_delete_script_block(window, cx);
                } else {
                    this.confirm_delete_block(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &MergeSelectedBlocks, _, cx| {
                if this.state.current_script.is_some() {
                    if let Err(err) = this.state.merge_selected_script_blocks(Utc::now()) {
                        eprintln!("merge failed: {err:#}");
                    }
                    this.invalidate_script_editor_inputs();
                } else if let Err(err) = this.state.merge_selected_blocks(Utc::now()) {
                    eprintln!("merge failed: {err:#}");
                    this.invalidate_editor_inputs();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveFocusedBlockUp, _, cx| {
                if this.state.current_script.is_some() {
                    if let Some(i) = this.state.script_focus.selected_index()
                        && let Err(err) = this.state.swap_script_block_at(i, true, Utc::now())
                    {
                        eprintln!("swap up failed: {err:#}");
                    }
                    this.invalidate_script_editor_inputs();
                } else if let Some(i) = this.state.block_focus.selected_index()
                    && let Err(err) = this.state.swap_block_at(i, true, Utc::now())
                {
                    eprintln!("swap up failed: {err:#}");
                    this.invalidate_editor_inputs();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveFocusedBlockDown, _, cx| {
                if this.state.current_script.is_some() {
                    if let Some(i) = this.state.script_focus.selected_index()
                        && let Err(err) = this.state.swap_script_block_at(i, false, Utc::now())
                    {
                        eprintln!("swap down failed: {err:#}");
                    }
                    this.invalidate_script_editor_inputs();
                } else if let Some(i) = this.state.block_focus.selected_index()
                    && let Err(err) = this.state.swap_block_at(i, false, Utc::now())
                {
                    eprintln!("swap down failed: {err:#}");
                    this.invalidate_editor_inputs();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &SetTypeNarration, _, cx| {
                this.set_focused_block_type(BlockType::Narration, cx);
            }))
            .on_action(cx.listener(|this, _: &SetTypeAside, _, cx| {
                this.set_focused_block_type(BlockType::Aside, cx);
            }))
            .on_action(cx.listener(|this, _: &SetTypeDialogue, _, cx| {
                this.set_focused_block_type(BlockType::Dialogue, cx);
            }))
            .on_action(cx.listener(|this, _: &SetTypeThought, _, cx| {
                this.set_focused_block_type(BlockType::Thought, cx);
            }))
            .on_action(cx.listener(|this, _: &SetTypeSceneBreak, _, cx| {
                this.set_focused_block_type(BlockType::SceneBreak, cx);
            }))
            .on_action(cx.listener(|this, _: &SetTypeNote, _, cx| {
                this.set_focused_block_type(BlockType::Note, cx);
            }))
            .on_action(cx.listener(|this, _: &NewOutlineEntry, window, cx| {
                this.prompt_new_outline(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RenameOutlineEntry, window, cx| {
                this.prompt_rename_outline(window, cx);
            }))
            .on_action(cx.listener(|this, _: &DeleteOutlineEntry, window, cx| {
                this.confirm_delete_outline(window, cx);
            }))
            .on_action(cx.listener(|this, _: &top_bar::NewProject, window, cx| {
                this.prompt_new_project(window, cx);
            }))
            .on_action(cx.listener(|this, _: &top_bar::OpenProject, window, cx| {
                this.prompt_open_project(window, cx);
            }))
            .on_action(cx.listener(|this, _: &top_bar::QuitApp, _, cx| {
                this.quit_app(cx);
            }))
            .on_action(cx.listener(|this, _: &top_bar::ToggleSidebar, _, cx| {
                this.state.toggle_sidebar();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &top_bar::ToggleAiPanel, _, cx| {
                this.state.toggle_ai_panel();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &top_bar::OpenSettings, window, cx| {
                this.prompt_settings(window, cx);
            }))
            .on_action(cx.listener(|this, action: &top_bar::OpenRecentAt, _, cx| {
                if let Err(err) = this.open_recent_at(action.0) {
                    eprintln!("open recent failed: {err:#}");
                }
                cx.notify();
            }))
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
                                    .child(sidebar::render_sidebar(self, window, cx)),
                            )
                            .child(resizable_panel().child(if self.state.current_outline.is_some() {
                                editor::render_outline_form(self, window, cx).into_any_element()
                            } else {
                                let has_doc = self.state.current_chapter.is_some()
                                    || self.state.current_script.is_some();
                                let preview_open =
                                    self.state.ui.preview_panel_open && has_doc;
                                let preview_label = if self.state.ui.preview_panel_open {
                                    "隐藏预览"
                                } else {
                                    "预览"
                                };

                                let editor_core = v_flex()
                                    .id("editor-pane")
                                    .size_full()
                                    .p_4()
                                    .gap_3()
                                    .child(if let Some(input) = self.title_input.as_ref() {
                                        div()
                                            .w_full()
                                            .child(Input::new(input).into_any_element())
                                            .into_any_element()
                                    } else {
                                        div()
                                            .text_lg()
                                            .font_bold()
                                            .child(editor_title)
                                            .into_any_element()
                                    })
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                Button::new("save-now")
                                                    .small()
                                                    .label("保存")
                                                    .disabled(
                                                        self.state.current_chapter.is_none()
                                                            && self.state.current_script.is_none()
                                                            && self.state.current_outline.is_none(),
                                                    )
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.save_document(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("toggle-preview")
                                                    .small()
                                                    .label(preview_label)
                                                    .disabled(!has_doc)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.state.toggle_preview_panel();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(
                                                        "提示：Enter 分割 · Shift+Enter 块内换行 · Cmd/Ctrl+S 保存",
                                                    ),
                                            ),
                                    )
                                    .child(if self.state.current_script.is_some() {
                                        editor::render_script_list(self, window, cx)
                                            .into_any_element()
                                    } else {
                                        editor::render_block_list(self, window, cx).into_any_element()
                                    });

                                if preview_open {
                                    div()
                                        .size_full()
                                        .child(
                                            h_resizable("editor-with-preview")
                                                .child(
                                                    resizable_panel().child(editor_core),
                                                )
                                                .child(
                                                    resizable_panel()
                                                        .size(px(300.))
                                                        .size_range(px(200.)..px(480.))
                                                        .child(editor::render_preview_panel(
                                                            self, window, cx,
                                                        )),
                                                ),
                                        )
                                        .into_any_element()
                                } else {
                                    editor_core.into_any_element()
                                }
                            }))
                            .child(
                                resizable_panel()
                                    .visible(self.state.ui.ai_panel_open)
                                    .size(px(280.))
                                    .size_range(px(220.)..px(420.))
                                    .child(ai_panel::render_ai_panel(self, window, cx)),
                            ),
                    ),
            )
            .child(status_bar::render_status_bar(&self.state, cx)),
            )
            .children(dialog_layer)
            .children(sheet_layer)
            .children(notification_layer)
    }
}
