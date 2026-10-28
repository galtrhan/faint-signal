//! Pluggable media backend. RingRTC is optional until you build it locally.

use crate::CallError;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingRtcStatus {
    /// Feature off or library not linked.
    Unavailable,
    /// Linked and ready to create call objects.
    Ready,
}

/// Audio (and later video) media path for one call.
#[async_trait]
pub trait MediaEngine: Send + Sync {
    fn status(&self) -> RingRtcStatus;

    async fn start_outgoing(&self, call_id: u64, audio_only: bool) -> Result<Vec<u8>, CallError>;
    async fn accept_incoming(&self, call_id: u64, offer: &[u8]) -> Result<Vec<u8>, CallError>;
    async fn apply_remote_signal(&self, call_id: u64, payload: &[u8]) -> Result<(), CallError>;
    async fn set_muted(&self, call_id: u64, muted: bool) -> Result<(), CallError>;
    async fn hangup(&self, call_id: u64) -> Result<(), CallError>;
}

/// Stand-in engine so messaging and UI work before RingRTC is linked.
pub struct NullMediaEngine;

#[async_trait]
impl MediaEngine for NullMediaEngine {
    fn status(&self) -> RingRtcStatus {
        RingRtcStatus::Unavailable
    }

    async fn start_outgoing(&self, _call_id: u64, audio_only: bool) -> Result<Vec<u8>, CallError> {
        if !audio_only {
            return Err(CallError::VideoOnHold);
        }
        Err(CallError::RingRtcUnavailable)
    }

    async fn accept_incoming(&self, _call_id: u64, _offer: &[u8]) -> Result<Vec<u8>, CallError> {
        Err(CallError::RingRtcUnavailable)
    }

    async fn apply_remote_signal(&self, _call_id: u64, _payload: &[u8]) -> Result<(), CallError> {
        Err(CallError::RingRtcUnavailable)
    }

    async fn set_muted(&self, _call_id: u64, _muted: bool) -> Result<(), CallError> {
        Err(CallError::RingRtcUnavailable)
    }

    async fn hangup(&self, _call_id: u64) -> Result<(), CallError> {
        Ok(())
    }
}
