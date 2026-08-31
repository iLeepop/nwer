use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{Block, BlockType, OutlineCategory, ScriptBlock, ScriptBlockType};

/// 写工具经 EffectPolicy 落地前的统一意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiIntent {
    CreateBlock {
        intent_id: Uuid,
        chapter_id: Uuid,
        block_type: BlockType,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speaker: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_block_id: Option<Uuid>,
    },
    ReplaceBlocks {
        intent_id: Uuid,
        chapter_id: Uuid,
        target_ids: Vec<Uuid>,
        blocks: Vec<Block>,
    },
    CreateOutlineEntry {
        intent_id: Uuid,
        category: OutlineCategory,
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fields: Option<BTreeMap<String, String>>,
    },
    UpdateOutlineEntry {
        intent_id: Uuid,
        id: Uuid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        category: Option<OutlineCategory>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fields: Option<BTreeMap<String, String>>,
    },
    CreateScript {
        intent_id: Uuid,
        title: String,
    },
    AppendScriptBlocks {
        intent_id: Uuid,
        script_id: Uuid,
        blocks: Vec<ScriptBlock>,
    },
    UpdateScriptBlock {
        intent_id: Uuid,
        script_id: Uuid,
        block_id: Uuid,
        #[serde(flatten)]
        fields: ScriptBlockUpdateFields,
    },
}

impl AiIntent {
    pub fn intent_id(&self) -> Uuid {
        match self {
            Self::CreateBlock { intent_id, .. }
            | Self::ReplaceBlocks { intent_id, .. }
            | Self::CreateOutlineEntry { intent_id, .. }
            | Self::UpdateOutlineEntry { intent_id, .. }
            | Self::CreateScript { intent_id, .. }
            | Self::AppendScriptBlocks { intent_id, .. }
            | Self::UpdateScriptBlock { intent_id, .. } => *intent_id,
        }
    }
}

/// 写工具更新剧本块时的可选字段（部分更新）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptBlockUpdateFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub block_type: Option<ScriptBlockType>,
}

/// 写工具执行结果：提案、已应用或错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    Proposed,
    Applied,
    Error,
}

/// 写工具返回给模型的 JSON 回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolReceipt {
    pub status: IntentStatus,
    pub intent_id: Uuid,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BlockType;
    use uuid::Uuid;

    #[test]
    fn tool_receipt_proposed_serializes() {
        let receipt = ToolReceipt {
            status: IntentStatus::Proposed,
            intent_id: Uuid::nil(),
            summary: "创建叙述块".into(),
            error: None,
        };
        let v = serde_json::to_value(&receipt).unwrap();
        assert_eq!(v["status"], "proposed");
        assert_eq!(v["summary"], "创建叙述块");
    }

    #[test]
    fn create_block_intent_holds_fields() {
        let id = Uuid::nil();
        let intent = AiIntent::CreateBlock {
            intent_id: id,
            chapter_id: id,
            block_type: BlockType::Narration,
            content: "你好".into(),
            speaker: None,
            after_block_id: None,
        };
        assert_eq!(intent.intent_id(), id);
    }
}
