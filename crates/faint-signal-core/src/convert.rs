//! Map presage content into app-store rows and call signals.

use faint_signal_calls::{CallDirection, CallSignal, CallSignalKind};
use faint_signal_store::{ConversationRow, MessageRow};
use presage::libsignal_service::content::{Content, ContentBody, DataMessage};
use presage::libsignal_service::prelude::Uuid;
use presage::proto::sync_message::Content as SyncContent;
use presage::store::{ContentExt, Thread};
use tracing::debug;

#[derive(Debug)]
pub enum Incoming {
    Message {
        conversation: ConversationRow,
        message: MessageRow,
        /// Parsed once here so session can resolve titles without a second TryFrom.
        thread: Option<Thread>,
    },
    Call(CallSignal),
}

pub fn content_to_incoming(content: &Content) -> Incoming {
    if let Some(call) = try_call_signal(content) {
        return Incoming::Call(call);
    }

    let (conversation_id, kind, title, thread) = conversation_from_content(content);
    let sync_sent = is_sync_sent(content);

    let body = match &content.body {
        ContentBody::DataMessage(DataMessage { body, .. }) => body.clone(),
        ContentBody::SynchronizeMessage(sync) => sync_body(sync),
        _ => None,
    };

    let sender = content.metadata.sender.raw_uuid().to_string();
    let sent_at = content.timestamp() as i64;
    Incoming::Message {
        conversation: ConversationRow {
            id: conversation_id,
            kind,
            title,
            last_message_at: Some(sent_at),
            unread: !sync_sent,
        },
        message: MessageRow {
            id: Uuid::new_v4(),
            conversation_id,
            sender,
            body,
            sent_at,
            direction: if sync_sent {
                "out".into()
            } else {
                "in".into()
            },
        },
        thread,
    }
}

fn is_sync_sent(content: &Content) -> bool {
    matches!(
        &content.body,
        ContentBody::SynchronizeMessage(sync)
            if matches!(sync.content.as_ref(), Some(SyncContent::Sent(_)))
    )
}

fn group_placeholder(master_key: &[u8]) -> (Uuid, String, String) {
    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, master_key);
    (
        id,
        "group".into(),
        format!("group:{}", &id.to_string()[..8]),
    )
}

fn conversation_from_content(content: &Content) -> (Uuid, String, String, Option<Thread>) {
    match Thread::try_from(content) {
        Ok(thread) => {
            let (id, kind, title) = match &thread {
                Thread::Contact(service_id) => {
                    let id = service_id.raw_uuid();
                    (id, "dm".into(), id.to_string())
                }
                Thread::Group(master_key) => group_placeholder(master_key),
            };
            (id, kind, title, Some(thread))
        }
        // Thread::try_from only fails on invalid ids/keys, not on ordinary DMs.
        Err(_) => {
            let id = content.metadata.sender.raw_uuid();
            (id, "dm".into(), id.to_string(), None)
        }
    }
}

fn sync_body(sync: &presage::proto::SyncMessage) -> Option<String> {
    match sync.content.as_ref() {
        Some(SyncContent::Sent(sent)) => sent
            .message
            .as_ref()
            .and_then(|m| m.body.clone())
            .or_else(|| {
                sent.edit_message
                    .as_ref()
                    .and_then(|e| e.data_message.as_ref())
                    .and_then(|m| m.body.clone())
            }),
        _ => None,
    }
}

fn try_call_signal(content: &Content) -> Option<CallSignal> {
    let ContentBody::CallMessage(call) = &content.body else {
        return None;
    };

    let kind = if call.offer.is_some() {
        CallSignalKind::Offer
    } else if call.answer.is_some() {
        CallSignalKind::Answer
    } else if call.hangup.is_some() {
        CallSignalKind::Hangup
    } else if !call.ice_update.is_empty() {
        CallSignalKind::IceUpdate
    } else if call.busy.is_some() {
        CallSignalKind::Busy
    } else {
        CallSignalKind::Opaque
    };

    let peer = content.metadata.sender.raw_uuid();
    let call_id = call
        .offer
        .as_ref()
        .and_then(|o| o.id)
        .or_else(|| call.answer.as_ref().and_then(|a| a.id))
        .or_else(|| call.hangup.as_ref().and_then(|h| h.id))
        .unwrap_or(0);
    let wants_video = call
        .offer
        .as_ref()
        .and_then(|o| o.r#type)
        .map(|t| t == 1)
        .unwrap_or(false);

    debug!(?kind, call_id, %peer, wants_video, "parsed call message");
    Some(CallSignal {
        kind,
        call_id,
        peer,
        direction: CallDirection::Incoming,
        payload: Vec::new(),
        wants_video,
    })
}
