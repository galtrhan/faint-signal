//! Call messages that ride on the normal Signal encrypted channel.
//!
//! Field names mirror what official clients exchange for 1:1 voice. Exact
//! protobuf mapping is filled in as we pin RingRTC / Desktop versions
//! (see `docs/calling.md`).

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallSignalKind {
    Offer,
    Answer,
    IceUpdate,
    Hangup,
    Busy,
    Opaque,
}

/// One signaling payload to send or that we received.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSignal {
    pub kind: CallSignalKind,
    pub call_id: u64,
    pub peer: Uuid,
    pub direction: CallDirection,
    /// Opaque bytes from RingRTC / WebRTC (SDP, ICE, hangup reason, …).
    #[serde(with = "payload_b64")]
    pub payload: Vec<u8>,
    /// True when the peer asked for video. FaintSignal v1 rejects these.
    #[serde(default)]
    pub wants_video: bool,
}

mod payload_b64 {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        B64.decode(text.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}
