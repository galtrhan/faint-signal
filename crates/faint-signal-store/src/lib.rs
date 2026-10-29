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

    /// Apply many contact display names in one persist. Creates missing DM rows.
    /// Returns how many titles changed (including new rows).
    pub fn apply_contact_titles(
        &self,
        titles: impl IntoIterator<Item = (Uuid, String)>,
    ) -> Result<u32> {
        let mut applied = 0u32;
        self.mutate_and_persist(|state| {
            for (id, title) in titles {
                if title.is_empty() {
                    continue;
                }
                if apply_title_in(state, id, title) {
                    applied += 1;
                }
            }
        })?;
        Ok(applied)
    }

    /// Human display name if one is already stored (not a UUID / `group:` placeholder).
    pub fn display_title(&self, id: Uuid) -> Option<String> {
        let state = self.state.lock();
        state
            .conversations
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.clone())
            .filter(|t| !is_placeholder_title(t))
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
        let title = prefer_title(&existing.title, &row.title);
        let last_message_at = match (existing.last_message_at, row.last_message_at) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => b.or(a),
        };
        *existing = ConversationRow {
            id: row.id,
            kind: row.kind.clone(),
            title,
            last_message_at,
            unread: row.unread,
        };
    } else {
        state.conversations.push(row.clone());
    }
}

/// Prefer a human name over a UUID / `group:…` placeholder.
fn prefer_title(existing: &str, incoming: &str) -> String {
    if incoming.is_empty() {
        return existing.to_string();
    }
    let existing_placeholder = is_placeholder_title(existing);
    let incoming_placeholder = is_placeholder_title(incoming);
    if !existing_placeholder && incoming_placeholder {
        existing.to_string()
    } else {
        incoming.to_string()
    }
}

fn is_placeholder_title(s: &str) -> bool {
    s.is_empty() || Uuid::parse_str(s).is_ok() || s.starts_with("group:")
}

/// Update or insert a DM title. Returns true when the stored title changed.
fn apply_title_in(state: &mut DiskState, id: Uuid, title: String) -> bool {
    if let Some(existing) = state.conversations.iter_mut().find(|c| c.id == id) {
        let next = prefer_title(&existing.title, &title);
        if next == existing.title {
            return false;
        }
        existing.title = next;
        true
    } else {
        state.conversations.push(ConversationRow {
            id,
            kind: "dm".into(),
            title,
            last_message_at: None,
            unread: false,
        });
        true
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
