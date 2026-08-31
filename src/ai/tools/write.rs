use std::collections::BTreeMap;

use chrono::Utc;
use rai_l::agent::core::{ToolParameters, ToolRegister};
use serde::Deserialize;
use uuid::Uuid;

use crate::ai::{EffectPolicy, AiIntent, IntentStatus, ScriptBlockUpdateFields, ToolReceipt};
use crate::models::{
    Block, BlockType, OutlineCategory, ScriptBlock, ScriptBlockType,
};

use super::SharedCtx;
use super::SharedAiCtx;

pub(crate) type ToolResult = Result<serde_json::Value, Box<dyn std::error::Error>>;

#[derive(Debug, Deserialize)]
struct CreateBlockArgs {
    chapter_id: Uuid,
    block_type: BlockType,
    content: String,
    #[serde(default)]
    speaker: Option<String>,
    #[serde(default)]
    after_block_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct BlockInput {
    #[serde(rename = "type")]
    block_type: BlockType,
    content: String,
    #[serde(default)]
    speaker: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProposeReplaceBlocksArgs {
    chapter_id: Uuid,
    target_ids: Vec<Uuid>,
    blocks: Vec<BlockInput>,
}

#[derive(Debug, Deserialize)]
struct CreateChapterDirectoryArgs {
    #[serde(default)]
    parent_rel: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateChapterFileArgs {
    #[serde(default)]
    parent_rel: String,
    name: String,
    title: String,
}

#[derive(Debug, Deserialize)]
struct CreateOutlineEntryArgs {
    category: OutlineCategory,
    key: String,
    #[serde(default)]
    fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct UpdateOutlineEntryArgs {
    id: Uuid,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    category: Option<OutlineCategory>,
    #[serde(default)]
    fields: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct CreateScriptArgs {
    title: String,
    #[serde(default)]
    parent_rel: String,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScriptBlockInput {
    #[serde(rename = "type")]
    block_type: ScriptBlockType,
    content: String,
    #[serde(default)]
    character: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AppendScriptBlocksArgs {
    script_id: Uuid,
    blocks: Vec<ScriptBlockInput>,
}

#[derive(Debug, Deserialize)]
struct UpdateScriptBlockArgs {
    script_id: Uuid,
    block_id: Uuid,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    character: Option<String>,
    #[serde(default, rename = "type")]
    block_type: Option<ScriptBlockType>,
}

pub(crate) fn new_intent_id() -> Uuid {
    Uuid::now_v7()
}

pub(crate) fn error_receipt(summary: impl Into<String>, error: impl Into<String>) -> ToolResult {
    Ok(serde_json::to_value(ToolReceipt {
        status: IntentStatus::Error,
        intent_id: new_intent_id(),
        summary: summary.into(),
        error: Some(error.into()),
    })?)
}

fn validate_chapter_speaker(block_type: BlockType, speaker: &Option<String>) -> Option<String> {
    if !block_type.allows_speaker() {
        return None;
    }
    match speaker {
        Some(s) if !s.trim().is_empty() => None,
        _ => Some(format!(
            "{label} 块必须指定 speaker",
            label = block_type.label()
        )),
    }
}

fn validate_script_character(
    block_type: ScriptBlockType,
    character: &Option<String>,
) -> Option<String> {
    if !block_type.allows_character() {
        return None;
    }
    match character {
        Some(s) if !s.trim().is_empty() => None,
        _ => Some(format!(
            "{label} 块必须指定 character",
            label = block_type.label()
        )),
    }
}

fn block_input_to_block(input: BlockInput) -> Result<Block, String> {
    if let Some(err) = validate_chapter_speaker(input.block_type, &input.speaker) {
        return Err(err);
    }
    let now = Utc::now();
    let mut block = Block::new(input.block_type, input.content, now);
    if let Some(s) = input.speaker {
        if input.block_type.allows_speaker() {
            block.speaker = Some(s);
        }
    }
    Ok(block)
}

fn script_block_input_to_block(input: ScriptBlockInput) -> Result<ScriptBlock, String> {
    if let Some(err) = validate_script_character(input.block_type, &input.character) {
        return Err(err);
    }
    let now = Utc::now();
    let mut block = ScriptBlock::new(input.block_type, input.content, now);
    if let Some(c) = input.character {
        if input.block_type.allows_character() {
            block.character = Some(c);
        }
    }
    Ok(block)
}

pub(crate) fn apply_via_policy(ctx: &SharedCtx, intent: AiIntent) -> ToolResult {
    let mut guard = ctx.lock().map_err(|e| e.to_string())?;
    let auto_apply = guard.policy.auto_apply;
    let SharedAiCtx {
        mutator,
        proposals,
        ..
    } = &mut *guard;
    let receipt = EffectPolicy::new(auto_apply).apply(intent, proposals, mutator);
    Ok(serde_json::to_value(receipt)?)
}

pub fn build_write_tools(ctx: SharedCtx, reg: &mut ToolRegister) {
    {
        let ctx = ctx.clone();
        reg.register(
            "create_block",
            "在指定章节创建正文块",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                    "block_type": {
                        "type": "string",
                        "enum": ["narration", "aside", "dialogue", "thought", "scene_break", "note"]
                    },
                    "content": { "type": "string" },
                    "speaker": { "type": "string" },
                    "after_block_id": { "type": "string", "format": "uuid" },
                },
                "required": ["chapter_id", "block_type", "content"],
            })),
            move |args: CreateBlockArgs| {
                let ctx = ctx.clone();
                async move {
                    if let Some(err) = validate_chapter_speaker(args.block_type, &args.speaker) {
                        return error_receipt("创建块", err);
                    }
                    let intent = AiIntent::CreateBlock {
                        intent_id: new_intent_id(),
                        chapter_id: args.chapter_id,
                        block_type: args.block_type,
                        content: args.content,
                        speaker: args.speaker,
                        after_block_id: args.after_block_id,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "propose_replace_blocks",
            "替换章节中指定块（产出 ReplaceBlocks 意图）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                    "target_ids": {
                        "type": "array",
                        "items": { "type": "string", "format": "uuid" },
                    },
                    "blocks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                    "enum": ["narration", "aside", "dialogue", "thought", "scene_break", "note"]
                                },
                                "content": { "type": "string" },
                                "speaker": { "type": "string" },
                            },
                            "required": ["type", "content"],
                        },
                    },
                },
                "required": ["chapter_id", "target_ids", "blocks"],
            })),
            move |args: ProposeReplaceBlocksArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.target_ids.is_empty() {
                        return error_receipt("替换块", "target_ids 不能为空");
                    }
                    if args.blocks.is_empty() {
                        return error_receipt("替换块", "blocks 不能为空");
                    }
                    let mut blocks = Vec::with_capacity(args.blocks.len());
                    for input in args.blocks {
                        match block_input_to_block(input) {
                            Ok(b) => blocks.push(b),
                            Err(e) => return error_receipt("替换块", e),
                        }
                    }
                    let intent = AiIntent::ReplaceBlocks {
                        intent_id: new_intent_id(),
                        chapter_id: args.chapter_id,
                        target_ids: args.target_ids,
                        blocks,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "create_chapter_directory",
            "在章节树 parent_rel（空=根）下创建子目录；name 须为合法存储名（无路径分隔符）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "parent_rel": {
                        "type": "string",
                        "description": "相对 chapters/ 的父目录，空字符串表示根",
                    },
                    "name": { "type": "string" },
                },
                "required": ["name"],
            })),
            move |args: CreateChapterDirectoryArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.name.trim().is_empty() {
                        return error_receipt("创建章节目录", "name 不能为空");
                    }
                    let intent = AiIntent::CreateChapterDirectory {
                        intent_id: new_intent_id(),
                        parent_rel: args.parent_rel,
                        name: args.name,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "create_chapter_file",
            "在指定目录下创建章节正文文件；返回提案含 chapter_id，可再 create_block / propose_replace_blocks",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "parent_rel": {
                        "type": "string",
                        "description": "相对 chapters/ 的父目录，空字符串表示根",
                    },
                    "name": {
                        "type": "string",
                        "description": "文件名（不含 .json），如 ch-001开篇",
                    },
                    "title": { "type": "string", "description": "章节显示标题" },
                },
                "required": ["name", "title"],
            })),
            move |args: CreateChapterFileArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.name.trim().is_empty() {
                        return error_receipt("创建章节文件", "name 不能为空");
                    }
                    if args.title.trim().is_empty() {
                        return error_receipt("创建章节文件", "title 不能为空");
                    }
                    let chapter_id = new_intent_id();
                    let intent = AiIntent::CreateChapterFile {
                        intent_id: new_intent_id(),
                        chapter_id,
                        parent_rel: args.parent_rel,
                        name: args.name,
                        title: args.title,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "create_outline_entry",
            "创建大纲条目",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "enum": ["角色", "背景", "场景", "事件", "杂项"],
                    },
                    "key": { "type": "string" },
                    "fields": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                    },
                },
                "required": ["category", "key"],
            })),
            move |args: CreateOutlineEntryArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.key.trim().is_empty() {
                        return error_receipt("创建大纲条目", "key 不能为空");
                    }
                    let intent = AiIntent::CreateOutlineEntry {
                        intent_id: new_intent_id(),
                        category: args.category,
                        key: args.key,
                        fields: args.fields,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "update_outline_entry",
            "更新大纲条目",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "format": "uuid" },
                    "key": { "type": "string" },
                    "category": {
                        "type": "string",
                        "enum": ["角色", "背景", "场景", "事件", "杂项"],
                    },
                    "fields": {
                        "type": "object",
                        "additionalProperties": { "type": "string" },
                    },
                },
                "required": ["id"],
            })),
            move |args: UpdateOutlineEntryArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::UpdateOutlineEntry {
                        intent_id: new_intent_id(),
                        id: args.id,
                        key: args.key,
                        category: args.category,
                        fields: args.fields,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "create_script",
            "创建新剧本；可选 parent_rel / name（相对 scripts/）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "parent_rel": {
                        "type": "string",
                        "description": "相对 scripts/ 的父目录，空字符串表示根",
                    },
                    "name": {
                        "type": "string",
                        "description": "文件名（不含 .json）；省略则由系统生成",
                    },
                },
                "required": ["title"],
            })),
            move |args: CreateScriptArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.title.trim().is_empty() {
                        return error_receipt("创建剧本", "title 不能为空");
                    }
                    if let Some(ref name) = args.name {
                        if name.trim().is_empty() {
                            return error_receipt("创建剧本", "name 不能为空");
                        }
                    }
                    let intent = AiIntent::CreateScript {
                        intent_id: new_intent_id(),
                        title: args.title,
                        parent_rel: args.parent_rel,
                        script_id: None,
                        name: args.name,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "append_script_blocks",
            "向剧本追加块",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "script_id": { "type": "string", "format": "uuid" },
                    "blocks": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "type": {
                                    "type": "string",
                                    "enum": [
                                        "scene_heading", "action", "character", "dialogue",
                                        "transition", "camera", "music", "mood", "note"
                                    ],
                                },
                                "content": { "type": "string" },
                                "character": { "type": "string" },
                            },
                            "required": ["type", "content"],
                        },
                    },
                },
                "required": ["script_id", "blocks"],
            })),
            move |args: AppendScriptBlocksArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.blocks.is_empty() {
                        return error_receipt("追加剧本块", "blocks 不能为空");
                    }
                    let mut blocks = Vec::with_capacity(args.blocks.len());
                    for input in args.blocks {
                        match script_block_input_to_block(input) {
                            Ok(b) => blocks.push(b),
                            Err(e) => return error_receipt("追加剧本块", e),
                        }
                    }
                    let intent = AiIntent::AppendScriptBlocks {
                        intent_id: new_intent_id(),
                        script_id: args.script_id,
                        blocks,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "update_script_block",
            "更新剧本块（部分字段）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "script_id": { "type": "string", "format": "uuid" },
                    "block_id": { "type": "string", "format": "uuid" },
                    "content": { "type": "string" },
                    "character": { "type": "string" },
                    "type": {
                        "type": "string",
                        "enum": [
                            "scene_heading", "action", "character", "dialogue",
                            "transition", "camera", "music", "mood", "note"
                        ],
                    },
                },
                "required": ["script_id", "block_id"],
            })),
            move |args: UpdateScriptBlockArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.content.is_none() && args.character.is_none() && args.block_type.is_none()
                    {
                        return error_receipt("更新剧本块", "至少提供一个待更新字段");
                    }
                    if let Some(block_type) = args.block_type {
                        if let Some(err) =
                            validate_script_character(block_type, &args.character)
                        {
                            return error_receipt("更新剧本块", err);
                        }
                    }
                    let intent = AiIntent::UpdateScriptBlock {
                        intent_id: new_intent_id(),
                        script_id: args.script_id,
                        block_id: args.block_id,
                        fields: ScriptBlockUpdateFields {
                            content: args.content,
                            character: args.character,
                            block_type: args.block_type,
                        },
                    };
                    apply_via_policy(&ctx, intent)
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

    fn sample_chapter_id() -> Uuid {
        Uuid::from_u128(1)
    }

    #[tokio::test]
    async fn create_block_tool_proposes_when_auto_apply_off() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(false)));
        let reg = build_all_tools(ctx.clone());
        let chapter_id = sample_chapter_id();
        let out = reg
            .call(
                "create_block",
                serde_json::json!({
                    "chapter_id": chapter_id,
                    "block_type": "narration",
                    "content": "新段落",
                }),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["status"], "proposed");
        assert_eq!(ctx.lock().unwrap().proposals.len(), 1);
    }

    #[tokio::test]
    async fn create_block_tool_applies_when_auto_apply_on() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(true)));
        let reg = build_all_tools(ctx.clone());
        let chapter_id = sample_chapter_id();
        let initial_len = ctx
            .lock()
            .unwrap()
            .mutator
            .chapter_blocks(chapter_id)
            .unwrap()
            .len();
        let out = reg
            .call(
                "create_block",
                serde_json::json!({
                    "chapter_id": chapter_id,
                    "block_type": "narration",
                    "content": "新段落",
                }),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["status"], "applied");
        let guard = ctx.lock().unwrap();
        let blocks = guard.mutator.chapter_blocks(chapter_id).unwrap();
        assert!(blocks.len() >= initial_len + 1);
    }

    #[tokio::test]
    async fn create_block_dialogue_without_speaker_returns_error() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(false)));
        let reg = build_all_tools(ctx.clone());
        let out = reg
            .call(
                "create_block",
                serde_json::json!({
                    "chapter_id": sample_chapter_id(),
                    "block_type": "dialogue",
                    "content": "你好",
                }),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["status"], "error");
        assert!(out["error"].as_str().unwrap().contains("speaker"));
        assert_eq!(ctx.lock().unwrap().proposals.len(), 0);
    }

    #[tokio::test]
    async fn create_block_thought_without_speaker_returns_error() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(false)));
        let mut reg = ToolRegister::new();
        build_write_tools(ctx.clone(), &mut reg);
        let out = reg
            .call(
                "create_block",
                serde_json::json!({
                    "chapter_id": sample_chapter_id(),
                    "block_type": "thought",
                    "content": "内心独白",
                }),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["status"], "error");
        assert_eq!(ctx.lock().unwrap().proposals.len(), 0);
    }
}
