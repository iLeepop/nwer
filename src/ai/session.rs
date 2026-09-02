use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rai_l::llm::Message;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai::{
    AiAgentKind, AiChatMessage, AiChatRole, AiMaxTokenTier, InMemoryMutator, ProposalStore,
};

/// 会话内 LLM 历史保留条数上限。
pub const MAX_LLM_HISTORY_MESSAGES: usize = 40;
/// 每项目最多保留的会话文件数。
pub const MAX_SESSIONS_PER_PROJECT: usize = 50;

/// 持久化的焦点快照（用于检测焦点变更）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredLeanFocus {
    Chapter { id: Uuid, title: String },
    Script { id: Uuid, title: String },
}

/// 列表页轻量视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSessionSummary {
    pub id: Uuid,
    pub title: String,
    pub updated_at: DateTime<Utc>,
}

/// 裁剪 LLM 消息历史，保留最近 `keep` 条。
pub fn truncate_llm_history(mut messages: Vec<Message>, keep: usize) -> Vec<Message> {
    if messages.len() > keep {
        messages.drain(..messages.len() - keep);
    }
    messages
}

/// 一条可持久化的 AI 对话会话。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSession {
    pub id: Uuid,
    pub title: String,
    pub agent: AiAgentKind,
    pub max_token_tier: AiMaxTokenTier,
    pub auto_apply: bool,
    pub ui_messages: Vec<AiChatMessage>,
    pub llm_messages: Vec<Message>,
    pub proposals: ProposalStore,
    /// 上一轮成功 run 时的焦点（用于增量 lean 注入）。
    #[serde(default)]
    pub last_lean_focus: Option<StoredLeanFocus>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AiSession {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::now_v7(),
            title: "新对话".into(),
            agent: AiAgentKind::default(),
            max_token_tier: AiMaxTokenTier::default(),
            auto_apply: false,
            ui_messages: Vec::new(),
            llm_messages: Vec::new(),
            proposals: ProposalStore::default(),
            last_lean_focus: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn touch(&mut self, now: DateTime<Utc>) {
        self.updated_at = now;
    }

    pub fn refresh_title_from_messages(&mut self) {
        if let Some(first) = self
            .ui_messages
            .iter()
            .find(|m| m.role == AiChatRole::User)
        {
            self.title = truncate_title(&first.text);
        }
    }

    pub fn is_first_llm_turn(&self) -> bool {
        self.llm_messages.is_empty()
    }
}

fn truncate_title(text: &str) -> String {
    let trimmed = text.trim();
    let char_count = trimmed.chars().count();
    if char_count <= 32 {
        trimmed.to_string()
    } else {
        format!("{}…", trimmed.chars().take(32).collect::<String>())
    }
}

/// 管理项目内多个 AI 会话；Phase 1 UI 仅暴露 active 会话。
#[derive(Debug, Clone)]
pub struct AiSessionManager {
    sessions: HashMap<Uuid, AiSession>,
    active_id: Option<Uuid>,
    pub busy: bool,
    pub streaming: bool,
    pub status_message: Option<String>,
    pub context_refs: Vec<crate::ai::AiContextRef>,
    pub proposals_expanded: bool,
    pub mutator: InMemoryMutator,
    empty_proposals: ProposalStore,
}

impl Default for AiSessionManager {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            active_id: None,
            busy: false,
            streaming: false,
            status_message: None,
            context_refs: Vec::new(),
            proposals_expanded: true,
            mutator: InMemoryMutator::new(),
            empty_proposals: ProposalStore::default(),
        }
    }
}

impl AiSessionManager {
    pub fn active(&self) -> Option<&AiSession> {
        self.active_id.and_then(|id| self.sessions.get(&id))
    }

    pub fn active_mut(&mut self) -> Option<&mut AiSession> {
        let id = self.active_id?;
        self.sessions.get_mut(&id)
    }

    pub fn active_id(&self) -> Option<Uuid> {
        self.active_id
    }

    /// 若无 active 会话则创建并选中。
    pub fn ensure_active(&mut self, now: DateTime<Utc>) -> &mut AiSession {
        if self.active_id.is_none()
            || !self
                .active_id
                .is_some_and(|id| self.sessions.contains_key(&id))
        {
            let session = AiSession::new(now);
            let id = session.id;
            self.sessions.insert(id, session);
            self.active_id = Some(id);
        }
        self.sessions
            .get_mut(&self.active_id.expect("just inserted"))
            .expect("session exists")
    }

    /// 从磁盘加载后替换内存态并选中给定 active。
    pub fn replace_loaded(&mut self, sessions: Vec<AiSession>, active_id: Option<Uuid>) {
        self.sessions = sessions.into_iter().map(|s| (s.id, s)).collect();
        self.active_id = active_id.filter(|id| self.sessions.contains_key(id));
        if self.active_id.is_none() {
            self.active_id = self
                .sessions
                .values()
                .max_by_key(|s| s.updated_at)
                .map(|s| s.id);
        }
        self.busy = false;
        self.streaming = false;
        self.context_refs.clear();
    }

    pub fn take_sessions(&mut self) -> Vec<AiSession> {
        self.sessions.drain().map(|(_, s)| s).collect()
    }

    /// 按 `updated_at` 降序列出会话摘要。
    pub fn list_summaries(&self) -> Vec<AiSessionSummary> {
        let mut list: Vec<AiSessionSummary> = self
            .sessions
            .values()
            .map(|s| AiSessionSummary {
                id: s.id,
                title: s.title.clone(),
                updated_at: s.updated_at,
            })
            .collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    pub fn active_title(&self) -> String {
        self.active()
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "新对话".into())
    }

    /// 新建会话并设为 active。
    pub fn create_session(&mut self, now: DateTime<Utc>) -> Uuid {
        let session = AiSession::new(now);
        let id = session.id;
        self.sessions.insert(id, session);
        self.active_id = Some(id);
        self.context_refs.clear();
        id
    }

    /// 切换到已有会话（生成中应由调用方拒绝）。
    pub fn switch_to(&mut self, id: Uuid) -> anyhow::Result<()> {
        if !self.sessions.contains_key(&id) {
            anyhow::bail!("session {id} not found");
        }
        self.active_id = Some(id);
        self.context_refs.clear();
        Ok(())
    }

    /// 从内存移除会话；若删的是 active 则选中 updated_at 最新者。
    pub fn remove_session(&mut self, id: Uuid) -> Option<AiSession> {
        let removed = self.sessions.remove(&id);
        if self.active_id == Some(id) {
            self.active_id = self
                .sessions
                .values()
                .max_by_key(|s| s.updated_at)
                .map(|s| s.id);
        }
        removed
    }

    /// 重命名 active 会话。
    pub fn rename_active(&mut self, title: impl Into<String>, now: DateTime<Utc>) -> anyhow::Result<()> {
        let title = title.into();
        let trimmed = title.trim();
        if trimmed.is_empty() {
            anyhow::bail!("会话标题不能为空");
        }
        let session = self
            .active_mut()
            .ok_or_else(|| anyhow::anyhow!("no active session"))?;
        session.title = trimmed.to_string();
        session.touch(now);
        Ok(())
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    // —— 面板访问器（委托 active 会话）——

    pub fn auto_apply(&self) -> bool {
        self.active().map(|s| s.auto_apply).unwrap_or(false)
    }

    pub fn max_token_tier(&self) -> AiMaxTokenTier {
        self.active()
            .map(|s| s.max_token_tier)
            .unwrap_or_default()
    }

    pub fn agent(&self) -> AiAgentKind {
        self.active().map(|s| s.agent).unwrap_or_default()
    }

    pub fn messages(&self) -> &[AiChatMessage] {
        static EMPTY: [AiChatMessage; 0] = [];
        self.active()
            .map(|s| s.ui_messages.as_slice())
            .unwrap_or(&EMPTY)
    }

    pub fn llm_messages(&self) -> &[Message] {
        static EMPTY: [Message; 0] = [];
        self.active()
            .map(|s| s.llm_messages.as_slice())
            .unwrap_or(&EMPTY)
    }

    pub fn proposals(&self) -> &ProposalStore {
        self.active()
            .map(|s| &s.proposals)
            .unwrap_or(&self.empty_proposals)
    }

    pub fn proposals_mut(&mut self) -> &mut ProposalStore {
        &mut self.ensure_active(Utc::now()).proposals
    }

    pub fn set_auto_apply(&mut self, auto_apply: bool, now: DateTime<Utc>) {
        let session = self.ensure_active(now);
        session.auto_apply = auto_apply;
        session.touch(now);
    }

    pub fn set_max_token_tier(&mut self, tier: AiMaxTokenTier, now: DateTime<Utc>) {
        let session = self.ensure_active(now);
        session.max_token_tier = tier;
        session.touch(now);
    }

    pub fn set_agent(&mut self, agent: AiAgentKind, now: DateTime<Utc>) {
        let session = self.ensure_active(now);
        session.agent = agent;
        session.touch(now);
    }

    pub fn set_llm_messages(&mut self, messages: Vec<Message>, now: DateTime<Utc>) {
        let session = self.ensure_active(now);
        session.llm_messages = messages;
        session.touch(now);
    }

    /// 无项目目录时，经内存 mutator 应用单条提案。
    pub fn apply_proposal_in_memory(
        &mut self,
        intent_id: uuid::Uuid,
    ) -> anyhow::Result<()> {
        use crate::ai::apply_proposal;
        let active_id = self
            .active_id
            .ok_or_else(|| anyhow::anyhow!("no ai session"))?;
        let session = self
            .sessions
            .get_mut(&active_id)
            .ok_or_else(|| anyhow::anyhow!("active session missing"))?;
        apply_proposal(intent_id, &mut session.proposals, &mut self.mutator)
    }

    /// 无项目目录时，经内存 mutator 批量应用提案。
    pub fn apply_all_in_memory(&mut self) -> anyhow::Result<()> {
        use crate::ai::apply_all;
        let active_id = self
            .active_id
            .ok_or_else(|| anyhow::anyhow!("no ai session"))?;
        let session = self
            .sessions
            .get_mut(&active_id)
            .ok_or_else(|| anyhow::anyhow!("active session missing"))?;
        apply_all(&mut session.proposals, &mut self.mutator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap()
    }

    #[test]
    fn ensure_active_creates_session() {
        let mut mgr = AiSessionManager::default();
        assert!(mgr.active().is_none());
        mgr.ensure_active(now());
        assert!(mgr.active().is_some());
        assert_eq!(mgr.active().unwrap().title, "新对话");
    }

    #[test]
    fn truncate_title_long_text() {
        let long = "这是一段超过三十二个汉字的中文标题用来测试截断逻辑是否正常工作以及额外后缀";
        let t = truncate_title(long);
        assert!(t.chars().count() <= 33);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn replace_loaded_picks_latest_active() {
        let mut mgr = AiSessionManager::default();
        let mut older = AiSession::new(now());
        older.updated_at = now();
        let mut newer = AiSession::new(now());
        newer.updated_at = now() + chrono::Duration::seconds(60);
        let newer_id = newer.id;
        mgr.replace_loaded(vec![older, newer], None);
        assert_eq!(mgr.active_id(), Some(newer_id));
    }

    #[test]
    fn create_and_switch_sessions() {
        let mut mgr = AiSessionManager::default();
        let id1 = mgr.create_session(now());
        mgr.ensure_active(now()).ui_messages.push(AiChatMessage {
            role: AiChatRole::User,
            text: "一".into(),
        });
        let id2 = mgr.create_session(now());
        assert_eq!(mgr.active_id(), Some(id2));
        assert_ne!(id1, id2);
        mgr.switch_to(id1).unwrap();
        assert_eq!(mgr.active_id(), Some(id1));
        assert_eq!(mgr.messages()[0].text, "一");
    }

    #[test]
    fn truncate_llm_history_keeps_tail() {
        use rai_l::llm::{Message, Role};
        let msgs: Vec<Message> = (0..5)
            .map(|i| Message::new(Role::User, format!("m{i}")))
            .collect();
        let kept = truncate_llm_history(msgs, 3);
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[0].text, "m2");
    }
}
