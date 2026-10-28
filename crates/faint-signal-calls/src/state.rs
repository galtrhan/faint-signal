//! Call state machine for 1:1 voice.

use crate::engine::MediaEngine;
use crate::signaling::{CallDirection, CallSignal, CallSignalKind};
use crate::CallError;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

pub type CallId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPhase {
    Idle,
    Dialing,
    Ringing,
    Connecting,
    Active,
    Ending,
}

#[derive(Debug, Clone)]
pub struct CallSnapshot {
    pub phase: CallPhase,
    pub call_id: Option<CallId>,
    pub peer: Option<Uuid>,
    pub direction: Option<CallDirection>,
    pub muted: bool,
}

impl CallSnapshot {
    pub fn idle() -> Self {
        Self {
            phase: CallPhase::Idle,
            call_id: None,
            peer: None,
            direction: None,
            muted: false,
        }
    }
}

impl Default for CallSnapshot {
    fn default() -> Self {
        Self::idle()
    }
}

#[derive(Debug, Clone)]
pub enum CallEvent {
    Phase(CallSnapshot),
    /// Signaling bytes the core must encrypt and send to `peer`.
    OutboundSignal(CallSignal),
    Error(String),
}

struct Active {
    call_id: CallId,
    peer: Uuid,
    direction: CallDirection,
    phase: CallPhase,
    muted: bool,
}

/// Owns the active call (at most one in v1) and talks to a [`MediaEngine`].
pub struct CallManager {
    engine: Arc<dyn MediaEngine>,
    active: RwLock<Option<Active>>,
    events: broadcast::Sender<CallEvent>,
    next_id: RwLock<u64>,
}

impl CallManager {
    pub fn new(engine: Arc<dyn MediaEngine>) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            engine,
            active: RwLock::new(None),
            events,
            next_id: RwLock::new(1),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CallEvent> {
        self.events.subscribe()
    }

    pub async fn snapshot(&self) -> CallSnapshot {
        let active = self.active.read().await;
        match active.as_ref() {
            None => CallSnapshot::idle(),
            Some(a) => CallSnapshot {
                phase: a.phase,
                call_id: Some(a.call_id),
                peer: Some(a.peer),
                direction: Some(a.direction),
                muted: a.muted,
            },
        }
    }

    async fn publish(&self, snap: CallSnapshot) {
        let _ = self.events.send(CallEvent::Phase(snap));
    }

    pub async fn start_outgoing(&self, peer: Uuid) -> Result<(), CallError> {
        {
            let active = self.active.read().await;
            if active.is_some() {
                return Err(CallError::AlreadyActive);
            }
        }
        let call_id = {
            let mut n = self.next_id.write().await;
            let id = *n;
            *n += 1;
            id
        };
        let offer = self.engine.start_outgoing(call_id, true).await?;
        {
            let mut active = self.active.write().await;
            *active = Some(Active {
                call_id,
                peer,
                direction: CallDirection::Outgoing,
                phase: CallPhase::Dialing,
                muted: false,
            });
        }
        let signal = CallSignal {
            kind: CallSignalKind::Offer,
            call_id,
            peer,
            direction: CallDirection::Outgoing,
            payload: offer,
            wants_video: false,
        };
        let _ = self.events.send(CallEvent::OutboundSignal(signal));
        self.publish(self.snapshot().await).await;
        info!(%peer, call_id, "outgoing voice call started");
        Ok(())
    }

    pub async fn handle_inbound(&self, signal: CallSignal) -> Result<(), CallError> {
        if signal.wants_video {
            warn!(call_id = signal.call_id, "rejecting video call (on hold)");
            return Err(CallError::VideoOnHold);
        }
        match signal.kind {
            CallSignalKind::Offer => {
                {
                    let active = self.active.read().await;
                    if active.is_some() {
                        let busy = CallSignal {
                            kind: CallSignalKind::Busy,
                            call_id: signal.call_id,
                            peer: signal.peer,
                            direction: CallDirection::Outgoing,
                            payload: Vec::new(),
                            wants_video: false,
                        };
                        let _ = self.events.send(CallEvent::OutboundSignal(busy));
                        return Err(CallError::AlreadyActive);
                    }
                }
                {
                    let mut active = self.active.write().await;
                    *active = Some(Active {
                        call_id: signal.call_id,
                        peer: signal.peer,
                        direction: CallDirection::Incoming,
                        phase: CallPhase::Ringing,
                        muted: false,
                    });
                }
                self.publish(self.snapshot().await).await;
                Ok(())
            }
            CallSignalKind::Answer | CallSignalKind::IceUpdate | CallSignalKind::Opaque => {
                self.engine
                    .apply_remote_signal(signal.call_id, &signal.payload)
                    .await?;
                let mut active = self.active.write().await;
                if let Some(a) = active.as_mut() {
                    if a.call_id == signal.call_id {
                        a.phase = CallPhase::Active;
                    }
                }
                drop(active);
                self.publish(self.snapshot().await).await;
                Ok(())
            }
            CallSignalKind::Hangup | CallSignalKind::Busy => {
                self.hangup_inner().await?;
                Ok(())
            }
        }
    }

    pub async fn accept(&self) -> Result<(), CallError> {
        let (call_id, peer, offer_placeholder) = {
            let active = self.active.read().await;
            let a = active.as_ref().ok_or(CallError::NoActiveCall)?;
            if a.phase != CallPhase::Ringing {
                return Err(CallError::Other("call is not ringing".into()));
            }
            (a.call_id, a.peer, Vec::<u8>::new())
        };
        let answer = self
            .engine
            .accept_incoming(call_id, &offer_placeholder)
            .await?;
        {
            let mut active = self.active.write().await;
            if let Some(a) = active.as_mut() {
                a.phase = CallPhase::Connecting;
            }
        }
        let signal = CallSignal {
            kind: CallSignalKind::Answer,
            call_id,
            peer,
            direction: CallDirection::Outgoing,
            payload: answer,
            wants_video: false,
        };
        let _ = self.events.send(CallEvent::OutboundSignal(signal));
        self.publish(self.snapshot().await).await;
        Ok(())
    }

    pub async fn set_muted(&self, muted: bool) -> Result<(), CallError> {
        let call_id = {
            let mut active = self.active.write().await;
            let a = active.as_mut().ok_or(CallError::NoActiveCall)?;
            a.muted = muted;
            a.call_id
        };
        self.engine.set_muted(call_id, muted).await?;
        self.publish(self.snapshot().await).await;
        Ok(())
    }

    pub async fn hangup(&self) -> Result<(), CallError> {
        self.hangup_inner().await
    }

    async fn hangup_inner(&self) -> Result<(), CallError> {
        let maybe = {
            let mut active = self.active.write().await;
            active.take()
        };
        if let Some(a) = maybe {
            let _ = self.engine.hangup(a.call_id).await;
            let signal = CallSignal {
                kind: CallSignalKind::Hangup,
                call_id: a.call_id,
                peer: a.peer,
                direction: CallDirection::Outgoing,
                payload: Vec::new(),
                wants_video: false,
            };
            let _ = self.events.send(CallEvent::OutboundSignal(signal));
            info!(call_id = a.call_id, "call ended");
        }
        self.publish(self.snapshot().await).await;
        Ok(())
    }
}
