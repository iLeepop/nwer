use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ai::AiSession;
use crate::ai::{AiSessionSummary, MAX_SESSIONS_PER_PROJECT};
use crate::storage::atomic_write;

/// 项目内 AI 会话目录：`{project_dir}/.ai/sessions/`。
pub fn ai_sessions_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".ai").join("sessions")
}

fn ai_meta_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".ai")
}

fn session_path(project_dir: &Path, id: Uuid) -> PathBuf {
    ai_sessions_dir(project_dir).join(format!("{id}.json"))
}

fn active_index_path(project_dir: &Path) -> PathBuf {
    ai_meta_dir(project_dir).join("active.json")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AiActiveIndex {
    pub active_id: Option<Uuid>,
}

impl Default for AiActiveIndex {
    fn default() -> Self {
        Self { active_id: None }
    }
}

/// 加载项目下全部会话文件。
pub fn load_sessions(project_dir: &Path) -> Result<Vec<AiSession>> {
    let dir = ai_sessions_dir(project_dir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path)
            .with_context(|| format!("read session {}", path.display()))?;
        let session: AiSession = serde_json::from_slice(&bytes)
            .with_context(|| format!("parse session {}", path.display()))?;
        sessions.push(session);
    }
    Ok(sessions)
}

/// 读取上次 active 会话 id。
pub fn load_active_session_id(project_dir: &Path) -> Result<Option<Uuid>> {
    let path = active_index_path(project_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let index: AiActiveIndex = serde_json::from_slice(&bytes)?;
    Ok(index.active_id)
}

/// 写入 active 会话 id。
pub fn save_active_session_id(project_dir: &Path, active_id: Option<Uuid>) -> Result<()> {
    let path = active_index_path(project_dir);
    let index = AiActiveIndex { active_id };
    let json = serde_json::to_vec_pretty(&index)?;
    atomic_write(&path, json).with_context(|| format!("save active index {}", path.display()))?;
    Ok(())
}

/// 原子写入单个会话。
pub fn save_session(project_dir: &Path, session: &AiSession) -> Result<()> {
    let path = session_path(project_dir, session.id);
    let json = serde_json::to_vec_pretty(session)?;
    atomic_write(&path, json).with_context(|| format!("save session {}", path.display()))?;
    Ok(())
}

/// 删除磁盘上的会话文件。
pub fn delete_session_file(project_dir: &Path, id: Uuid) -> Result<()> {
    let path = session_path(project_dir, id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// 持久化当前 active 会话（若有）。
pub fn persist_session_if_active(
    project_dir: &Path,
    session: Option<&AiSession>,
) -> Result<()> {
    if let Some(s) = session {
        save_session(project_dir, s)?;
    }
    Ok(())
}

/// 持久化 active 会话 + active 索引。
pub fn persist_ai_state(project_dir: &Path, session: Option<&AiSession>, active_id: Option<Uuid>) -> Result<()> {
    persist_session_if_active(project_dir, session)?;
    save_active_session_id(project_dir, active_id)?;
    Ok(())
}

/// 删除超出 `keep` 的最旧会话文件，返回被删 id 列表。
pub fn prune_old_sessions(project_dir: &Path, keep: usize) -> Result<Vec<Uuid>> {
    let mut sessions = load_sessions(project_dir)?;
    if sessions.len() <= keep {
        return Ok(Vec::new());
    }
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut deleted = Vec::new();
    for session in sessions.into_iter().skip(keep) {
        delete_session_file(project_dir, session.id)?;
        deleted.push(session.id);
    }
    Ok(deleted)
}

/// 从磁盘加载会话摘要（按 updated_at 降序）。
pub fn list_session_summaries(project_dir: &Path) -> Result<Vec<AiSessionSummary>> {
    let mut list: Vec<AiSessionSummary> = load_sessions(project_dir)?
        .into_iter()
        .map(|s| AiSessionSummary {
            id: s.id,
            title: s.title,
            updated_at: s.updated_at,
        })
        .collect();
    list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::tempdir;

    use crate::ai::{AiChatMessage, AiChatRole};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap()
    }

    #[test]
    fn save_and_load_session_roundtrip() {
        let dir = tempdir().unwrap();
        let project_dir = dir.path().join("novel");
        fs::create_dir_all(&project_dir).unwrap();

        let mut session = AiSession::new(now());
        session.ui_messages.push(AiChatMessage {
            role: AiChatRole::User,
            text: "你好".into(),
        });
        session.refresh_title_from_messages();

        save_session(&project_dir, &session).unwrap();
        let loaded = load_sessions(&project_dir).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, session.id);
        assert_eq!(loaded[0].title, "你好");
        assert_eq!(loaded[0].ui_messages[0].text, "你好");
    }

    #[test]
    fn active_index_roundtrip() {
        let dir = tempdir().unwrap();
        let project_dir = dir.path().join("novel");
        fs::create_dir_all(&project_dir).unwrap();
        let id = Uuid::now_v7();
        save_active_session_id(&project_dir, Some(id)).unwrap();
        assert_eq!(load_active_session_id(&project_dir).unwrap(), Some(id));
    }

    #[test]
    fn prune_keeps_newest_sessions() {
        let dir = tempdir().unwrap();
        let project_dir = dir.path().join("novel");
        fs::create_dir_all(&project_dir).unwrap();
        let mut ids = Vec::new();
        for i in 0..3 {
            let mut s = AiSession::new(now());
            s.updated_at = now() + chrono::Duration::seconds(i);
            s.title = format!("s{i}");
            ids.push(s.id);
            save_session(&project_dir, &s).unwrap();
        }
        let deleted = prune_old_sessions(&project_dir, 2).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0], ids[0]);
        assert_eq!(load_sessions(&project_dir).unwrap().len(), 2);
    }

    #[test]
    fn delete_session_file_is_idempotent() {
        let dir = tempdir().unwrap();
        let project_dir = dir.path().join("novel");
        let id = Uuid::now_v7();
        delete_session_file(&project_dir, id).unwrap();
    }
}
