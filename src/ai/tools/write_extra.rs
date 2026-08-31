//! 补全写工具：元信息、块移动/删除、章节/剧本树、大纲删除。

use rai_l::agent::core::{ToolParameters, ToolRegister};
use serde::Deserialize;
use uuid::Uuid;

use crate::ai::AiIntent;
use crate::models::BlockType;

use super::write::{apply_via_policy, error_receipt, new_intent_id};
use super::SharedCtx;

#[derive(Debug, Deserialize)]
struct UpdateProjectMetaArgs {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    style_guide: Option<String>,
    #[serde(default)]
    synopsis: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateBlockArgs {
    chapter_id: Uuid,
    block_id: Uuid,
    #[serde(default)]
    content: Option<String>,
    #[serde(default, rename = "type")]
    block_type: Option<BlockType>,
    #[serde(default)]
    speaker: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteBlockArgs {
    chapter_id: Uuid,
    block_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct MoveBlockArgs {
    chapter_id: Uuid,
    block_id: Uuid,
    to_index: usize,
}

#[derive(Debug, Deserialize)]
struct RenameNodeArgs {
    rel_path: String,
    new_name: String,
}

#[derive(Debug, Deserialize)]
struct RelPathArgs {
    rel_path: String,
}

#[derive(Debug, Deserialize)]
struct MoveNodeArgs {
    rel_path: String,
    dest_parent_rel: String,
}

#[derive(Debug, Deserialize)]
struct MoveSiblingArgs {
    rel_path: String,
    /// -1 = up, 1 = down
    direction: i8,
}

#[derive(Debug, Deserialize)]
struct CopyChapterArgs {
    src_rel: String,
    dest_parent_rel: String,
    new_name: String,
}

#[derive(Debug, Deserialize)]
struct UpdateChapterTitleArgs {
    chapter_id: Uuid,
    title: String,
}

#[derive(Debug, Deserialize)]
struct CreateScriptDirectoryArgs {
    #[serde(default)]
    parent_rel: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CopyScriptArgs {
    src_rel: String,
    dest_parent_rel: String,
    new_name: String,
}

#[derive(Debug, Deserialize)]
struct UpdateScriptTitleArgs {
    script_id: Uuid,
    title: String,
}

#[derive(Debug, Deserialize)]
struct DeleteScriptBlockArgs {
    script_id: Uuid,
    block_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct MoveScriptBlockArgs {
    script_id: Uuid,
    block_id: Uuid,
    to_index: usize,
}

#[derive(Debug, Deserialize)]
struct DeleteOutlineEntryArgs {
    id: Uuid,
}

pub fn build_write_extra_tools(ctx: SharedCtx, reg: &mut ToolRegister) {
    {
        let ctx = ctx.clone();
        reg.register(
            "update_project_meta",
            "更新项目标题、风格指南或简介（至少一项）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "style_guide": { "type": "string" },
                    "synopsis": { "type": "string" },
                },
            })),
            move |args: UpdateProjectMetaArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.title.is_none()
                        && args.style_guide.is_none()
                        && args.synopsis.is_none()
                    {
                        return error_receipt("更新项目元信息", "至少提供一个待更新字段");
                    }
                    let intent = AiIntent::UpdateProjectMeta {
                        intent_id: new_intent_id(),
                        title: args.title,
                        style_guide: args.style_guide,
                        synopsis: args.synopsis,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "update_block",
            "更新章节正文块（部分字段）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                    "block_id": { "type": "string", "format": "uuid" },
                    "content": { "type": "string" },
                    "type": {
                        "type": "string",
                        "enum": ["narration", "aside", "dialogue", "thought", "scene_break", "note"]
                    },
                    "speaker": { "type": "string" },
                },
                "required": ["chapter_id", "block_id"],
            })),
            move |args: UpdateBlockArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.content.is_none()
                        && args.block_type.is_none()
                        && args.speaker.is_none()
                    {
                        return error_receipt("更新块", "至少提供一个待更新字段");
                    }
                    let intent = AiIntent::UpdateBlock {
                        intent_id: new_intent_id(),
                        chapter_id: args.chapter_id,
                        block_id: args.block_id,
                        content: args.content,
                        block_type: args.block_type,
                        speaker: args.speaker,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "delete_block",
            "删除章节中的正文块",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                    "block_id": { "type": "string", "format": "uuid" },
                },
                "required": ["chapter_id", "block_id"],
            })),
            move |args: DeleteBlockArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::DeleteBlock {
                        intent_id: new_intent_id(),
                        chapter_id: args.chapter_id,
                        block_id: args.block_id,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "move_block",
            "将章节正文块移动到指定下标",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                    "block_id": { "type": "string", "format": "uuid" },
                    "to_index": { "type": "integer", "minimum": 0 },
                },
                "required": ["chapter_id", "block_id", "to_index"],
            })),
            move |args: MoveBlockArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::MoveBlock {
                        intent_id: new_intent_id(),
                        chapter_id: args.chapter_id,
                        block_id: args.block_id,
                        to_index: args.to_index,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "rename_chapter_node",
            "重命名章节树节点（目录或文件，相对 chapters/）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "new_name": { "type": "string" },
                },
                "required": ["rel_path", "new_name"],
            })),
            move |args: RenameNodeArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.new_name.trim().is_empty() {
                        return error_receipt("重命名章节节点", "new_name 不能为空");
                    }
                    let intent = AiIntent::RenameChapterNode {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                        new_name: args.new_name,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "delete_chapter_node",
            "删除章节树节点（目录或文件）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                },
                "required": ["rel_path"],
            })),
            move |args: RelPathArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::DeleteChapterNode {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "move_chapter_node",
            "移动章节树节点到目标父目录",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "dest_parent_rel": { "type": "string" },
                },
                "required": ["rel_path", "dest_parent_rel"],
            })),
            move |args: MoveNodeArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::MoveChapterNode {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                        dest_parent_rel: args.dest_parent_rel,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "move_chapter_sibling",
            "在同级中上下移动章节节点；direction: -1 上 / 1 下",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "direction": { "type": "integer", "enum": [-1, 1] },
                },
                "required": ["rel_path", "direction"],
            })),
            move |args: MoveSiblingArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.direction != -1 && args.direction != 1 {
                        return error_receipt("移动章节同级", "direction 须为 -1 或 1");
                    }
                    let intent = AiIntent::MoveChapterSibling {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                        direction: args.direction,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "copy_chapter",
            "复制章节文件到目标父目录；返回提案含 new_chapter_id",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "src_rel": { "type": "string" },
                    "dest_parent_rel": { "type": "string" },
                    "new_name": { "type": "string" },
                },
                "required": ["src_rel", "dest_parent_rel", "new_name"],
            })),
            move |args: CopyChapterArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.new_name.trim().is_empty() {
                        return error_receipt("复制章节", "new_name 不能为空");
                    }
                    let intent = AiIntent::CopyChapter {
                        intent_id: new_intent_id(),
                        src_rel: args.src_rel,
                        dest_parent_rel: args.dest_parent_rel,
                        new_name: args.new_name,
                        new_chapter_id: Uuid::now_v7(),
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "update_chapter_title",
            "更新章节显示标题",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "chapter_id": { "type": "string", "format": "uuid" },
                    "title": { "type": "string" },
                },
                "required": ["chapter_id", "title"],
            })),
            move |args: UpdateChapterTitleArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.title.trim().is_empty() {
                        return error_receipt("更新章节标题", "title 不能为空");
                    }
                    let intent = AiIntent::UpdateChapterTitle {
                        intent_id: new_intent_id(),
                        chapter_id: args.chapter_id,
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
            "create_script_directory",
            "在剧本树 parent_rel（空=根）下创建子目录",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "parent_rel": {
                        "type": "string",
                        "description": "相对 scripts/ 的父目录，空字符串表示根",
                    },
                    "name": { "type": "string" },
                },
                "required": ["name"],
            })),
            move |args: CreateScriptDirectoryArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.name.trim().is_empty() {
                        return error_receipt("创建剧本目录", "name 不能为空");
                    }
                    let intent = AiIntent::CreateScriptDirectory {
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
            "rename_script_node",
            "重命名剧本树节点（目录或文件，相对 scripts/）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "new_name": { "type": "string" },
                },
                "required": ["rel_path", "new_name"],
            })),
            move |args: RenameNodeArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.new_name.trim().is_empty() {
                        return error_receipt("重命名剧本节点", "new_name 不能为空");
                    }
                    let intent = AiIntent::RenameScriptNode {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                        new_name: args.new_name,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "delete_script_node",
            "删除剧本树节点（目录或文件）",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                },
                "required": ["rel_path"],
            })),
            move |args: RelPathArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::DeleteScriptNode {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "move_script_node",
            "移动剧本树节点到目标父目录",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "dest_parent_rel": { "type": "string" },
                },
                "required": ["rel_path", "dest_parent_rel"],
            })),
            move |args: MoveNodeArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::MoveScriptNode {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                        dest_parent_rel: args.dest_parent_rel,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "move_script_sibling",
            "在同级中上下移动剧本节点；direction: -1 上 / 1 下",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "direction": { "type": "integer", "enum": [-1, 1] },
                },
                "required": ["rel_path", "direction"],
            })),
            move |args: MoveSiblingArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.direction != -1 && args.direction != 1 {
                        return error_receipt("移动剧本同级", "direction 须为 -1 或 1");
                    }
                    let intent = AiIntent::MoveScriptSibling {
                        intent_id: new_intent_id(),
                        rel_path: args.rel_path,
                        direction: args.direction,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "copy_script",
            "复制剧本文件到目标父目录；返回提案含 new_script_id",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "src_rel": { "type": "string" },
                    "dest_parent_rel": { "type": "string" },
                    "new_name": { "type": "string" },
                },
                "required": ["src_rel", "dest_parent_rel", "new_name"],
            })),
            move |args: CopyScriptArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.new_name.trim().is_empty() {
                        return error_receipt("复制剧本", "new_name 不能为空");
                    }
                    let intent = AiIntent::CopyScript {
                        intent_id: new_intent_id(),
                        src_rel: args.src_rel,
                        dest_parent_rel: args.dest_parent_rel,
                        new_name: args.new_name,
                        new_script_id: Uuid::now_v7(),
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "update_script_title",
            "更新剧本显示标题",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "script_id": { "type": "string", "format": "uuid" },
                    "title": { "type": "string" },
                },
                "required": ["script_id", "title"],
            })),
            move |args: UpdateScriptTitleArgs| {
                let ctx = ctx.clone();
                async move {
                    if args.title.trim().is_empty() {
                        return error_receipt("更新剧本标题", "title 不能为空");
                    }
                    let intent = AiIntent::UpdateScriptTitle {
                        intent_id: new_intent_id(),
                        script_id: args.script_id,
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
            "delete_script_block",
            "删除剧本中的块",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "script_id": { "type": "string", "format": "uuid" },
                    "block_id": { "type": "string", "format": "uuid" },
                },
                "required": ["script_id", "block_id"],
            })),
            move |args: DeleteScriptBlockArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::DeleteScriptBlock {
                        intent_id: new_intent_id(),
                        script_id: args.script_id,
                        block_id: args.block_id,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "move_script_block",
            "将剧本块移动到指定下标",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "script_id": { "type": "string", "format": "uuid" },
                    "block_id": { "type": "string", "format": "uuid" },
                    "to_index": { "type": "integer", "minimum": 0 },
                },
                "required": ["script_id", "block_id", "to_index"],
            })),
            move |args: MoveScriptBlockArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::MoveScriptBlock {
                        intent_id: new_intent_id(),
                        script_id: args.script_id,
                        block_id: args.block_id,
                        to_index: args.to_index,
                    };
                    apply_via_policy(&ctx, intent)
                }
            },
        );
    }

    {
        let ctx = ctx.clone();
        reg.register(
            "delete_outline_entry",
            "删除大纲条目",
            ToolParameters::new(serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "format": "uuid" },
                },
                "required": ["id"],
            })),
            move |args: DeleteOutlineEntryArgs| {
                let ctx = ctx.clone();
                async move {
                    let intent = AiIntent::DeleteOutlineEntry {
                        intent_id: new_intent_id(),
                        id: args.id,
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
    async fn delete_block_tool_proposes_when_auto_apply_off() {
        let ctx = Arc::new(Mutex::new(SharedAiCtx::with_sample_chapter(false)));
        let block_id = {
            let guard = ctx.lock().unwrap();
            guard
                .mutator
                .chapter_blocks(sample_chapter_id())
                .unwrap()[0]
                .id
        };
        let reg = build_all_tools(ctx.clone());
        let out = reg
            .call(
                "delete_block",
                serde_json::json!({
                    "chapter_id": sample_chapter_id(),
                    "block_id": block_id,
                }),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(out["status"], "proposed");
        assert_eq!(ctx.lock().unwrap().proposals.len(), 1);
    }
}
