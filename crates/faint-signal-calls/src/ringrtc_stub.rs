//! Placeholder for a real RingRTC-backed [`MediaEngine`].
//!
//! Enable with `--features ringrtc` after you link a local RingRTC build.
//! See `docs/calling.md`.

#![cfg(feature = "ringrtc")]

use crate::engine::{MediaEngine, RingRtcStatus};
use crate::CallError;
use async_trait::async_trait;

/// Skeleton engine. Replace method bodies with RingRTC FFI once linked.
pub struct RingRtcEngine;

#[async_trait]
impl MediaEngine for RingRtcEngine {
    fn status(&self) -> RingRtcStatus {
        RingRtcStatus::Unavailable
    }

    async fn start_outgoing(&self, _call_id: u64, audio_only: bool) -> Result<Vec<u8>, CallError> {
        if !audio_only {
            return Err(CallError::VideoOnHold);
        }
        Err(CallError::Other(
            "RingRtcEngine stub: wire Signal RingRTC here (docs/calling.md)".into(),
        ))
    }

    async fn accept_incoming(&self, _call_id: u64, _offer: &[u8]) -> Result<Vec<u8>, CallError> {
        Err(CallError::Other(
            "RingRtcEngine stub: wire Signal RingRTC here (docs/calling.md)".into(),
        ))
    }

    async fn apply_remote_signal(&self, _call_id: u64, _payload: &[u8]) -> Result<(), CallError> {
        Ok(())
    }

    async fn set_muted(&self, _call_id: u64, _muted: bool) -> Result<(), CallError> {
        Ok(())
    }

    async fn hangup(&self, _call_id: u64) -> Result<(), CallError> {
        Ok(())
    }
}
