use rai_l::agent::core::{ToolParameters, ToolRegister};
use serde::Deserialize;
use uuid::Uuid;

use super::SharedCtx;

#[derive(Debug, Deserialize)]
struct EmptyArgs {}

#[derive(Debug, Deserialize)]
struct ChapterIdArgs {
    chapter_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct ScriptIdArgs {
    script_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct OutlineIdArgs {
    id: Uuid,
}

pub fn build_read_tools(ctx: SharedCtx, reg: &mut ToolRegister) {
    {
        let ctx = ctx.clone();
        reg.register(
            "get_project_meta",
            "获取项目标题、风格指南与简介",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    let m = &guard.mutator;
                    Ok(serde_json::json!({
                        "title": m.title,
                        "style_guide": m.style_guide,
                        "synopsis": m.synopsis,
                    }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "list_chapters",
            "列出所有章节的 id 与标题",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    let chapters: Vec<_> = guard
                        .mutator
                        .list_chapters()
                        .into_iter()
                        .map(|(id, title)| {
                            let rel = guard.mutator.chapter_rel(id).unwrap_or("");
                            serde_json::json!({ "id": id, "title": title, "rel_path": rel })
                        })
                        .collect();
                    Ok(serde_json::json!({ "chapters": chapters }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "list_chapter_tree",
            "列出章节树：目录与文件的相对路径（相对 chapters/）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    let dirs: Vec<_> = guard
                        .mutator
                        .list_dirs()
                        .into_iter()
                        .map(|rel| serde_json::json!({ "kind": "directory", "rel_path": rel }))
                        .collect();
                    let chapters: Vec<_> = guard
                        .mutator
                        .list_chapters()
                        .into_iter()
                        .map(|(id, title)| {
                            serde_json::json!({
                                "kind": "chapter",
                                "id": id,
                                "title": title,
                                "rel_path": guard.mutator.chapter_rel(id).unwrap_or(""),
                            })
                        })
                        .collect();
                    Ok(serde_json::json!({ "dirs": dirs, "chapters": chapters }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "list_scripts",
            "列出所有剧本的 id 与标题",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    let scripts: Vec<_> = guard
                        .mutator
                        .list_scripts()
                        .into_iter()
                        .map(|(id, title)| {
                            let rel = guard.mutator.script_rel(id).unwrap_or("");
                            serde_json::json!({ "id": id, "title": title, "rel_path": rel })
                        })
                        .collect();
                    Ok(serde_json::json!({ "scripts": scripts }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "list_script_tree",
            "列出剧本树：目录与文件的相对路径（相对 scripts/）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    let dirs: Vec<_> = guard
                        .mutator
                        .list_script_dirs()
                        .into_iter()
                        .map(|rel| serde_json::json!({ "kind": "directory", "rel_path": rel }))
                        .collect();
                    let scripts: Vec<_> = guard
                        .mutator
                        .list_scripts()
                        .into_iter()
                        .map(|(id, title)| {
                            serde_json::json!({
                                "kind": "script",
                                "id": id,
                                "title": title,
                                "rel_path": guard.mutator.script_rel(id).unwrap_or(""),
                            })
                        })
                        .collect();
                    Ok(serde_json::json!({ "dirs": dirs, "scripts": scripts }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "get_outline_tree",
            "获取大纲条目列表（按分类与 key）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({ "entries": guard.mutator.get_outline() }))
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "get_outline_entry",
            "按 id 获取单个大纲条目",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "format": "uuid" },
                },
                "required": ["id"],
            })),
            move |args: OutlineIdArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    match guard.mutator.get_outline().iter().find(|e| e.id == args.id) {
                        Some(entry) => {
                            Ok(serde_json::to_value(entry).map_err(|e| e.to_string())?)
                        }
                        None => Ok(serde_json::json!({
                            "error": "not_found",
                            "id": args.id,
                        })),
                    }
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "get_chapter_blocks",
            "获取指定章节的正文块列表",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                },
                "required": ["chapter_id"],
            })),
            move |args: ChapterIdArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    match guard.mutator.chapter_blocks(args.chapter_id) {
                        Some(blocks) => Ok(serde_json::json!({ "blocks": blocks })),
                        None => Ok(serde_json::json!({
                            "error": "not_found",
                            "chapter_id": args.chapter_id,
                        })),
                    }
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "get_script",
            "获取指定剧本的全文块",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "script_id": { "type": "string", "format": "uuid" },
                },
                "required": ["script_id"],
            })),
            move |args: ScriptIdArgs| {
                let ctx = ctx.clone();
                async move {
                    let guard = ctx.lock().map_err(|e| e.to_string())?;
                    match guard.mutator.get_script(args.script_id) {
                        Some(script) => {
                            Ok(serde_json::to_value(script).map_err(|e| e.to_string())?)
                        }
                        None => Ok(serde_json::json!({
                            "error": "not_found",
                            "script_id": args.script_id,
                        })),
                    }
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "get_selection_context",
            "获取当前编辑器选区/焦点上下文",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let _ctx = ctx.clone();
                async move { Ok(serde_json::json!({})) }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tools::{SharedAiCtx, build_all_tools};
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    fn sample_chapter_id() -> Uuid {
        Uuid::from_u128(1)
    }

    #[tokio::test]
    async fn get_chapter_blocks_returns_json() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(false)));
        let reg = build_all_tools(ctx);
        let chapter_id = sample_chapter_id();
        let out = reg
            .call(
                "get_chapter_blocks",
                serde_json::json!({ "chapter_id": chapter_id }),
            )
            .await
            .unwrap()
            .unwrap();
        assert!(out["blocks"].as_array().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn get_project_meta_returns_title() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(false)));
        let mut reg = ToolRegister::new();
        build_read_tools(ctx, &mut reg);
        let out = reg
            .call("get_project_meta", serde_json::json!({}))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["title"].as_str().unwrap(), "测试项目");
    }
}
