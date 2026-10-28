use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRow {
    pub id: Uuid,
    /// `"dm"` or `"group"`.
    pub kind: String,
    pub title: String,
    pub last_message_at: Option<i64>,
    pub unread: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender: String,
    pub body: Option<String>,
    pub sent_at: i64,
    /// `"in"` or `"out"`.
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentRow {
    pub id: Uuid,
    pub message_id: Uuid,
    pub path: String,
    pub content_type: String,
    pub size_bytes: i64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct DiskState {
    pub conversations: Vec<ConversationRow>,
    pub messages: Vec<MessageRow>,
    pub attachments: Vec<AttachmentRow>,
}
