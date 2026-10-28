//! App-level message and conversation cache for FaintSignal.
//!
//! Protocol identity and session keys live in the presage SQLCipher store.
//! This crate holds UI-facing rows in a JSON file so we do not fight
//! `libsqlite3-sys` versions with presage.

mod paths;
mod schema;

pub use paths::{paths, FaintPaths};
pub use schema::{AttachmentRow, ConversationRow, MessageRow};

use anyhow::{Context, Result};
use parking_lot::Mutex;
use schema::DiskState;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Local UI / history database (separate from the presage Signal store).
pub struct AppStore {
    path: PathBuf,
    state: Mutex<DiskState>,
}

impl AppStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create store parent {}", parent.display()))?;
        }
        let path = path.as_ref().to_path_buf();
        let state = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("read app store {}", path.display()))?;
            serde_json::from_str(&raw).with_context(|| {
                format!(
                    "parse app store {} (rename or delete the file if it is corrupt)",
                    path.display()
                )
            })?
        } else {
            DiskState::default()
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn persist_snapshot(&self, state: DiskState) -> Result<()> {
        let raw = serde_json::to_string(&state)?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, raw)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    fn mutate_and_persist(&self, f: impl FnOnce(&mut DiskState)) -> Result<()> {
        let snapshot = {
            let mut state = self.state.lock();
            f(&mut state);
            state.clone()
        };
        self.persist_snapshot(snapshot)
    }

    pub fn upsert_conversation(&self, row: &ConversationRow) -> Result<()> {
        self.mutate_and_persist(|state| upsert_conversation_in(state, row))
    }

    pub fn list_conversations(&self, limit: u32) -> Result<Vec<ConversationRow>> {
        let state = self.state.lock();
        let mut rows = state.conversations.clone();
        rows.sort_by_key(|c| std::cmp::Reverse(c.last_message_at.unwrap_or(0)));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    pub fn insert_message(&self, row: &MessageRow) -> Result<()> {
        self.mutate_and_persist(|state| upsert_message_in(state, row))
    }

    /// One lock + one disk write for an inbound message and its conversation.
    pub fn upsert_inbound(
        &self,
        conversation: &ConversationRow,
        message: &MessageRow,
    ) -> Result<()> {
        self.mutate_and_persist(|state| {
            upsert_conversation_in(state, conversation);
            upsert_message_in(state, message);
        })
    }

    /// Page messages newest-first for a conversation.
    pub fn messages_page(
        &self,
        conversation_id: Uuid,
        before_sent_at: Option<i64>,
        limit: u32,
    ) -> Result<Vec<MessageRow>> {
        let state = self.state.lock();
        let mut rows: Vec<_> = state
            .messages
            .iter()
            .filter(|m| m.conversation_id == conversation_id)
            .filter(|m| before_sent_at.map(|b| m.sent_at < b).unwrap_or(true))
            .cloned()
            .collect();
        rows.sort_by_key(|m| std::cmp::Reverse(m.sent_at));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    pub fn record_attachment(&self, row: &AttachmentRow) -> Result<()> {
        self.mutate_and_persist(|state| {
            if let Some(existing) = state.attachments.iter_mut().find(|a| a.id == row.id) {
                *existing = row.clone();
            } else {
                state.attachments.push(row.clone());
            }
        })
    }
}

fn upsert_conversation_in(state: &mut DiskState, row: &ConversationRow) {
    if let Some(existing) = state.conversations.iter_mut().find(|c| c.id == row.id) {
        *existing = row.clone();
    } else {
        state.conversations.push(row.clone());
    }
}

fn upsert_message_in(state: &mut DiskState, row: &MessageRow) {
    if let Some(existing) = state.messages.iter_mut().find(|m| m.id == row.id) {
        *existing = row.clone();
    } else {
        state.messages.push(row.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn page_messages() {
        let dir = tempdir().unwrap();
        let store = AppStore::open(dir.path().join("app.json")).unwrap();
        let cid = Uuid::new_v4();
        store
            .upsert_inbound(
                &ConversationRow {
                    id: cid,
                    kind: "dm".into(),
                    title: "Test".into(),
                    last_message_at: Some(100),
                    unread: false,
                },
                &MessageRow {
                    id: Uuid::new_v4(),
                    conversation_id: cid,
                    sender: "me".into(),
                    body: Some("m0".into()),
                    sent_at: 0,
                    direction: "out".into(),
                },
            )
            .unwrap();
        for i in 1..5 {
            store
                .insert_message(&MessageRow {
                    id: Uuid::new_v4(),
                    conversation_id: cid,
                    sender: "me".into(),
                    body: Some(format!("m{i}")),
                    sent_at: i * 10,
                    direction: "out".into(),
                })
                .unwrap();
        }
        let page = store.messages_page(cid, None, 3).unwrap();
        assert_eq!(page.len(), 3);
        assert_eq!(page[0].sent_at, 40);
    }

    #[test]
    fn corrupt_json_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.json");
        std::fs::write(&path, "{not json").unwrap();
        assert!(AppStore::open(&path).is_err());
    }
}
