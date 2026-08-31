//! AppState ↔ AiSessionHost 桥接：配置、瘦上下文、读侧 hydrate、跑完后提案同步。

use anyhow::{bail, Result};
use rai_l::llm::{Provider, RaiLLM, RaiLLMArgs};
use uuid::Uuid;

use crate::ai::host::{LeanContext, LeanFocus, LeanSelection};
use crate::ai::mutator::InMemoryMutator;
use crate::ai::provider::{ChapterContext, ProjectContext};
use crate::ai::ui_state::AiMaxTokenTier;
use crate::ai::SharedAiCtx;
use crate::app::AppState;
use crate::storage::{AiSettings, ChapterTreeNode, ScriptTreeNode};

/// 高档固定值（高于 raiL `MAX_TOKENS_REASONING`，适合长文/多轮工具）。
pub const MAX_TOKENS_HIGH: u32 = 16_384;

/// 将设置中的 provider 字符串映射为 raiL Provider。
pub fn provider_from_settings(provider: &str) -> Provider {
    match provider.trim().to_ascii_lowercase().as_str() {
        "deepseek" => Provider::DEEPSEEK,
        "kimi" => Provider::KIMI,
        "ollama" => Provider::OLLAMA,
        "vllm" => Provider::VLLM,
        "local" => Provider::LOCAL,
        _ => Provider::DEEPSEEK,
    }
}

/// 按档位 + 供应商解析实际 max_tokens。
pub fn resolve_max_tokens(tier: AiMaxTokenTier, provider: &str) -> u32 {
    match tier {
        AiMaxTokenTier::Low => RaiLLM::MAX_TOKENS_SHORT,
        AiMaxTokenTier::High => MAX_TOKENS_HIGH,
        AiMaxTokenTier::Auto => match provider_from_settings(provider) {
            Provider::DEEPSEEK | Provider::KIMI => RaiLLM::MAX_TOKENS_REASONING,
            _ => RaiLLM::MAX_TOKENS_LONG,
        },
    }
}

/// 校验设置是否足以发起真实 LLM 调用。
pub fn validate_ai_ready(ai: &AiSettings) -> Result<()> {
    if ai.api_key.trim().is_empty() {
        bail!("请先在设置中填写 API Key");
    }
    if ai.model.trim().is_empty() {
        bail!("请先在设置中填写模型名称");
    }
    if !ai.base_url.trim().is_empty()
        && !(ai.base_url.starts_with("http://") || ai.base_url.starts_with("https://"))
    {
        bail!("默认调用地址须以 http:// 或 https:// 开头");
    }
    Ok(())
}

/// 由全局 AI 设置构造 RaiLLM。
pub fn build_llm(ai: &AiSettings) -> Result<RaiLLM> {
    validate_ai_ready(ai)?;
    let base_url = {
        let u = ai.base_url.trim();
        if u.is_empty() {
            None
        } else {
            Some(u.to_string())
        }
    };
    RaiLLMArgs::default()
        .with_provider(Some(provider_from_settings(&ai.provider)))
        .with_api_key(Some(ai.api_key.trim().to_string()))
        .with_base_url(base_url)
        .with_model_id(ai.model.trim())
        .build()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// 从当前 AppState 组装瘦上下文。
pub fn lean_context_from_app(state: &AppState) -> LeanContext {
    let project = match state.project.as_ref() {
        Some(p) => ProjectContext {
            project_id: p.id,
            title: p.title.clone(),
            style_guide: p.ai_context.style_guide.clone(),
            synopsis: p.ai_context.synopsis.clone(),
        },
        None => ProjectContext {
            project_id: Uuid::nil(),
            title: "未打开项目".into(),
            style_guide: String::new(),
            synopsis: String::new(),
        },
    };

    if let Some(script) = state.current_script.as_ref() {
        let selection = state
            .script_focus
            .selected_index()
            .and_then(|i| script.blocks.get(i))
            .map(|b| {
                vec![LeanSelection {
                    block_id: b.id,
                    block_type: b.block_type.label().to_string(),
                    text: b.content.clone(),
                }]
            })
            .unwrap_or_default();
        return LeanContext {
            project,
            focus: Some(LeanFocus::Script {
                id: script.id,
                title: script.title.clone(),
            }),
            selection,
        };
    }

    if let Some(chapter) = state.current_chapter.as_ref() {
        let mut selection = Vec::new();
        if let Some(multi) = state.block_multi_select.as_ref() {
            for i in multi.start..=multi.end {
                if let Some(b) = chapter.blocks.get(i) {
                    selection.push(LeanSelection {
                        block_id: b.id,
                        block_type: b.block_type.label().to_string(),
                        text: b.content.clone(),
                    });
                }
            }
        } else if let Some(i) = state.block_focus.selected_index() {
            if let Some(b) = chapter.blocks.get(i) {
                selection.push(LeanSelection {
                    block_id: b.id,
                    block_type: b.block_type.label().to_string(),
                    text: b.content.clone(),
                });
            }
        }
        return LeanContext {
            project,
            focus: Some(LeanFocus::Chapter {
                id: chapter.id,
                title: chapter.title.clone(),
            }),
            selection,
        };
    }

    LeanContext {
        project,
        focus: None,
        selection: Vec::new(),
    }
}

/// 将当前打开的项目内容灌入内存 mutator，供读工具使用。
/// 写工具在 Host 内一律走提案（SharedAiCtx.auto_apply=false），由面板确认或自动批量应用。
pub fn hydrate_shared_ctx(state: &AppState) -> SharedAiCtx {
    let mut mutator = InMemoryMutator::new();
    if let Some(p) = state.project.as_ref() {
        mutator.title = p.title.clone();
        mutator.style_guide = p.ai_context.style_guide.clone();
        mutator.synopsis = p.ai_context.synopsis.clone();
    }
    hydrate_chapter_tree(&mut mutator, &state.chapter_tree);
    hydrate_script_tree(&mut mutator, &state.script_tree);
    if let Some(ch) = state.current_chapter.as_ref() {
        mutator.upsert_chapter(ch.clone());
    }
    if let Some(script) = state.current_script.as_ref() {
        mutator.ensure_script(script.clone());
    }
    mutator.set_outline(state.outline_entries.clone());

    // Host 工具侧始终提案；面板「自动应用」在 run 结束后统一 apply_all。
    SharedAiCtx {
        mutator,
        proposals: Default::default(),
        policy: crate::ai::EffectPolicy::new(false),
        ui_commands: Vec::new(),
    }
}

fn hydrate_chapter_tree(mutator: &mut InMemoryMutator, nodes: &[ChapterTreeNode]) {
    for node in nodes {
        if node.is_directory() {
            mutator.ensure_dir(node.rel_path.clone());
            hydrate_chapter_tree(mutator, &node.children);
        } else if let Some(id) = node.chapter_id {
            mutator.set_chapter_rel(id, node.rel_path.clone());
            if mutator.get_chapter(id).is_none() {
                let title = node.title.clone().unwrap_or_else(|| node.name.clone());
                mutator.ensure_chapter(id, title);
            }
        }
    }
}

fn hydrate_script_tree(mutator: &mut InMemoryMutator, nodes: &[ScriptTreeNode]) {
    for node in nodes {
        if node.is_directory() {
            mutator.ensure_script_dir(node.rel_path.clone());
            hydrate_script_tree(mutator, &node.children);
        } else if let Some(id) = node.script_id {
            mutator.set_script_rel(id, node.rel_path.clone());
            if mutator.get_script(id).is_none() {
                let title = node.title.clone().unwrap_or_else(|| node.name.clone());
                let mut script = crate::models::Script::new(title, chrono::Utc::now());
                script.id = id;
                script.blocks.clear();
                mutator.ensure_script(script);
            }
        }
    }
}

/// 附带章节元信息（供测试断言）。
pub fn chapter_context_from_app(state: &AppState) -> Option<ChapterContext> {
    state.current_chapter.as_ref().map(|ch| ChapterContext {
        chapter_id: ch.id,
        title: ch.title.clone(),
        status: ch.meta.status.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ui_state::AiMaxTokenTier;
    use crate::storage::AiSettings;

    #[test]
    fn validate_requires_key_and_model() {
        let mut ai = AiSettings::default();
        assert!(validate_ai_ready(&ai).is_err());
        ai.api_key = "sk".into();
        assert!(validate_ai_ready(&ai).is_err());
        ai.model = "m".into();
        assert!(validate_ai_ready(&ai).is_ok());
    }

    #[test]
    fn provider_mapping() {
        assert!(matches!(
            provider_from_settings("kimi"),
            Provider::KIMI
        ));
        assert!(matches!(
            provider_from_settings("DeepSeek"),
            Provider::DEEPSEEK
        ));
    }

    #[test]
    fn resolve_max_tokens_tiers() {
        assert_eq!(
            resolve_max_tokens(AiMaxTokenTier::Low, "deepseek"),
            RaiLLM::MAX_TOKENS_SHORT
        );
        assert_eq!(
            resolve_max_tokens(AiMaxTokenTier::High, "ollama"),
            MAX_TOKENS_HIGH
        );
        assert_eq!(
            resolve_max_tokens(AiMaxTokenTier::Auto, "deepseek"),
            RaiLLM::MAX_TOKENS_REASONING
        );
        assert_eq!(
            resolve_max_tokens(AiMaxTokenTier::Auto, "kimi"),
            RaiLLM::MAX_TOKENS_REASONING
        );
        assert_eq!(
            resolve_max_tokens(AiMaxTokenTier::Auto, "ollama"),
            RaiLLM::MAX_TOKENS_LONG
        );
        assert_eq!(
            resolve_max_tokens(AiMaxTokenTier::Auto, "vllm"),
            RaiLLM::MAX_TOKENS_LONG
        );
    }
}
