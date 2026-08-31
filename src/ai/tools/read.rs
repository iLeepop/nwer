use std::sync::{Arc, Mutex};

use rai_l::agent::core::{ToolParameters, ToolRegister};
use serde::Deserialize;
use uuid::Uuid;

use crate::ai::InMemoryMutator;

pub type SharedMutator = Arc<Mutex<InMemoryMutator>>;

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

pub fn build_read_tools(state: SharedMutator) -> ToolRegister {
    let mut reg = ToolRegister::new();

    {
        let state = state.clone();
        reg.register(
            "get_project_meta",
            "获取项目标题、风格指南与简介",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let state = state.clone();
                async move {
                    let m = state.lock().map_err(|e| e.to_string())?;
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
        let state = state.clone();
        reg.register(
            "list_chapters",
            "列出所有章节的 id 与标题",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let state = state.clone();
                async move {
                    let m = state.lock().map_err(|e| e.to_string())?;
                    let chapters: Vec<_> = m
                        .list_chapters()
                        .into_iter()
                        .map(|(id, title)| serde_json::json!({ "id": id, "title": title }))
                        .collect();
                    Ok(serde_json::json!({ "chapters": chapters }))
                }
            },
        );
    }

    {
        let state = state.clone();
        reg.register(
            "list_scripts",
            "列出所有剧本的 id 与标题",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let state = state.clone();
                async move {
                    let m = state.lock().map_err(|e| e.to_string())?;
                    let scripts: Vec<_> = m
                        .list_scripts()
                        .into_iter()
                        .map(|(id, title)| serde_json::json!({ "id": id, "title": title }))
                        .collect();
                    Ok(serde_json::json!({ "scripts": scripts }))
                }
            },
        );
    }

    {
        let state = state.clone();
        reg.register(
            "get_outline_tree",
            "获取大纲条目列表（按分类与 key）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let state = state.clone();
                async move {
                    let m = state.lock().map_err(|e| e.to_string())?;
                    Ok(serde_json::json!({ "entries": m.get_outline() }))
                }
            },
        );
    }

    {
        let state = state.clone();
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
                let state = state.clone();
                async move {
                    let m = state.lock().map_err(|e| e.to_string())?;
                    match m.chapter_blocks(args.chapter_id) {
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
        let state = state.clone();
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
                let state = state.clone();
                async move {
                    let m = state.lock().map_err(|e| e.to_string())?;
                    match m.get_script(args.script_id) {
                        Some(script) => Ok(serde_json::to_value(script).map_err(|e| e.to_string())?),
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
        let state = state.clone();
        reg.register(
            "get_selection_context",
            "获取当前编辑器选区/焦点上下文",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            })),
            move |_: EmptyArgs| {
                let _state = state.clone();
                async move { Ok(serde_json::json!({})) }
            },
        );
    }

    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_chapter_id() -> Uuid {
        Uuid::from_u128(1)
    }

    #[tokio::test]
    async fn get_chapter_blocks_returns_json() {
        let state = Arc::new(Mutex::new(InMemoryMutator::with_sample_chapter()));
        let reg = build_read_tools(state.clone());
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
        let state = Arc::new(Mutex::new(InMemoryMutator::with_sample_chapter()));
        let reg = build_read_tools(state);
        let out = reg
            .call("get_project_meta", serde_json::json!({}))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["title"].as_str().unwrap(), "测试项目");
    }
}
