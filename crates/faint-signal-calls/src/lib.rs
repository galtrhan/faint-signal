//! Voice call signaling and media lifecycle for FaintSignal.
//!
//! v1 is **audio-only 1:1**. Video and group calls stay out of this crate's
//! public API until Phase 5.
//!
//! Media goes through [`MediaEngine`]. The default is [`NullMediaEngine`].
//! Wire Signal's RingRTC behind the `ringrtc` feature (see `docs/calling.md`).

mod engine;
mod signaling;
mod state;

#[cfg(feature = "ringrtc")]
mod ringrtc_stub;

pub use engine::{MediaEngine, NullMediaEngine, RingRtcStatus};
pub use signaling::{CallDirection, CallSignal, CallSignalKind};
pub use state::{CallEvent, CallId, CallManager, CallPhase, CallSnapshot};

#[cfg(feature = "ringrtc")]
pub use ringrtc_stub::RingRtcEngine;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CallError {
    #[error("no active call")]
    NoActiveCall,
    #[error("call already active")]
    AlreadyActive,
    #[error("video is on hold in v1")]
    VideoOnHold,
    #[error("group calling is on hold in v1")]
    GroupOnHold,
    #[error("ringrtc not linked; build with --features ringrtc after following docs/calling.md")]
    RingRtcUnavailable,
    #[error("{0}")]
    Other(String),
}
