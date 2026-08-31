//! 即时 UI 焦点工具：不经 EffectPolicy，直接入队 AiUiCommand。

use rai_l::agent::core::{ToolParameters, ToolRegister};
use serde::Deserialize;
use uuid::Uuid;

use crate::ai::AiUiCommand;

use super::SharedCtx;

#[derive(Debug, Deserialize)]
struct OpenChapterArgs {
    chapter_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct OpenScriptArgs {
    script_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct OpenOutlineArgs {
    outline_id: Uuid,
}

pub fn build_focus_tools(ctx: SharedCtx, reg: &mut ToolRegister) {
    {
        let ctx = ctx.clone();
        reg.register(
            "open_chapter",
            "打开并聚焦指定章节（即时生效，不经提案）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                },
                "required": ["chapter_id"],
            })),
            move |args: OpenChapterArgs| {
                let ctx = ctx.clone();
                async move {
                    let mut guard = ctx.lock().map_err(|e| e.to_string())?;
                    guard.ui_commands.push(AiUiCommand::OpenChapter {
                        chapter_id: args.chapter_id,
                    });
                    Ok(serde_json::json!({ "ok": true, "chapter_id": args.chapter_id }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "open_script",
            "打开并聚焦指定剧本（即时生效，不经提案）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "script_id": { "type": "string", "format": "uuid" },
                },
                "required": ["script_id"],
            })),
            move |args: OpenScriptArgs| {
                let ctx = ctx.clone();
                async move {
                    let mut guard = ctx.lock().map_err(|e| e.to_string())?;
                    guard.ui_commands.push(AiUiCommand::OpenScript {
                        script_id: args.script_id,
                    });
                    Ok(serde_json::json!({ "ok": true, "script_id": args.script_id }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "open_outline",
            "打开并聚焦指定大纲条目（即时生效，不经提案）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "outline_id": { "type": "string", "format": "uuid" },
                },
                "required": ["outline_id"],
            })),
            move |args: OpenOutlineArgs| {
                let ctx = ctx.clone();
                async move {
                    let mut guard = ctx.lock().map_err(|e| e.to_string())?;
                    guard.ui_commands.push(AiUiCommand::OpenOutline {
                        outline_id: args.outline_id,
                    });
                    Ok(serde_json::json!({ "ok": true, "outline_id": args.outline_id }))
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tools::{SharedAiCtx, build_all_tools};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn open_chapter_queues_ui_command() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(false)));
        let reg = build_all_tools(ctx.clone());
        let chapter_id = Uuid::from_u128(1);
        let out = reg
            .call(
                "open_chapter",
                serde_json::json!({ "chapter_id": chapter_id }),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["ok"], true);
        let cmds = ctx.lock().unwrap().take_ui_commands();
        assert_eq!(
            cmds,
            vec![AiUiCommand::OpenChapter { chapter_id }]
        );
        assert_eq!(ctx.lock().unwrap().proposals.len(), 0);
    }
}
