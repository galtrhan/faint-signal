//! Map presage content into app-store rows and call signals.

use faint_signal_calls::{CallDirection, CallSignal, CallSignalKind};
use faint_signal_store::{ConversationRow, MessageRow};
use presage::libsignal_service::content::{Content, ContentBody, DataMessage};
use presage::libsignal_service::prelude::Uuid;
use presage::proto::sync_message::Content as SyncContent;
use presage::store::ContentExt;
use tracing::debug;

#[derive(Debug)]
pub enum Incoming {
    Message {
        conversation: ConversationRow,
        message: MessageRow,
    },
    Call(CallSignal),
}

pub fn content_to_incoming(content: &Content) -> Incoming {
    if let Some(call) = try_call_signal(content) {
        return Incoming::Call(call);
    }

    let (conversation_id, kind, title) = conversation_from_content(content);

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
            unread: true,
        },
        message: MessageRow {
            id: Uuid::new_v4(),
            conversation_id,
            sender,
            body,
            sent_at,
            direction: "in".into(),
        },
    }
}

fn conversation_from_content(content: &Content) -> (Uuid, String, String) {
    if let ContentBody::DataMessage(DataMessage {
        group_v2: Some(g), ..
    }) = &content.body
    {
        if let Some(master_key) = g.master_key.as_ref() {
            let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, master_key);
            return (
                id,
                "group".into(),
                format!("group:{}", &id.to_string()[..8]),
            );
        }
    }

    let id = content.metadata.sender.raw_uuid();
    (id, "dm".into(), id.to_string())
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
