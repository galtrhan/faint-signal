//! Send-safe session handle. Presage's `Manager` is `!Send`, so it lives on a
//! dedicated thread with a `LocalSet`. The UI talks through channels.

use crate::session::{print_rss, Session, SessionConfig};
use anyhow::{anyhow, Result};
use faint_signal_calls::CallSnapshot;
use faint_signal_store::{paths, ConversationRow, FaintPaths, MessageRow};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

enum Command {
    Whoami(oneshot::Sender<Result<String>>),
    ListConversations {
        limit: u32,
        reply: oneshot::Sender<Result<Vec<ConversationRow>>>,
    },
    MessagesPage {
        conversation_id: Uuid,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<MessageRow>>>,
    },
    SendText {
        recipient: Uuid,
        body: String,
        reply: oneshot::Sender<Result<()>>,
    },
    ReceiveOnce(oneshot::Sender<Result<()>>),
    CallSnapshot(oneshot::Sender<CallSnapshot>),
    StartCall {
        peer: Uuid,
        reply: oneshot::Sender<Result<()>>,
    },
    Hangup(oneshot::Sender<Result<()>>),
    Accept(oneshot::Sender<Result<()>>),
    SetMuted {
        muted: bool,
        reply: oneshot::Sender<Result<()>>,
    },
}

/// Cloneable handle usable from iced / any Send future.
#[derive(Clone)]
pub struct SessionHandle {
    tx: mpsc::UnboundedSender<Command>,
    pub paths: FaintPaths,
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("data_dir", &self.paths.data_dir)
            .finish()
    }
}

impl SessionHandle {
    pub fn try_load(config: SessionConfig) -> Result<Self> {
        let faint_paths = paths()?;
        faint_paths.ensure_dirs()?;
        let (tx, rx) = mpsc::unbounded_channel();
        let paths_clone = faint_paths.clone();
        std::thread::Builder::new()
            .name("faintsignal-session".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("session runtime");
                let local = tokio::task::LocalSet::new();
                local.block_on(&rt, async move {
                    match Session::load(paths_clone.clone(), &config).await {
                        Ok(session) => run_actor(session, rx).await,
                        Err(e) => {
                            tracing::error!(error = %e, "session load failed on actor thread");
                            // Drain pending replies with errors.
                            let mut rx = rx;
                            while let Some(cmd) = rx.recv().await {
                                fail_cmd(cmd, anyhow!("session not loaded: {e}"));
                            }
                        }
                    }
                });
            })?;
        Ok(Self {
            tx,
            paths: faint_paths,
        })
    }

    pub fn link_device(config: SessionConfig) -> Result<Self> {
        let faint_paths = paths()?;
        faint_paths.ensure_dirs()?;
        let (tx, rx) = mpsc::unbounded_channel();
        let paths_clone = faint_paths.clone();
        std::thread::Builder::new()
            .name("faintsignal-link".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("link runtime");
                let local = tokio::task::LocalSet::new();
                local.block_on(&rt, async move {
                    print_rss("before-link-actor");
                    match Session::link_device(paths_clone.clone(), config).await {
                        Ok(session) => {
                            print_rss("after-link-actor");
                            run_actor(session, rx).await;
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "link failed");
                            let mut rx = rx;
                            while let Some(cmd) = rx.recv().await {
                                fail_cmd(cmd, anyhow!("link failed: {e}"));
                            }
                        }
                    }
                });
            })?;
        Ok(Self {
            tx,
            paths: faint_paths,
        })
    }

    async fn ask<T>(&self, build: impl FnOnce(oneshot::Sender<T>) -> Command) -> Result<T>
    where
        T: Send + 'static,
    {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(build(reply))
            .map_err(|_| anyhow!("session actor stopped"))?;
        rx.await
            .map_err(|_| anyhow!("session actor dropped reply"))
    }

    pub async fn whoami(&self) -> Result<String> {
        self.ask(Command::Whoami).await?
    }

    pub async fn list_conversations(&self, limit: u32) -> Result<Vec<ConversationRow>> {
        self.ask(|reply| Command::ListConversations { limit, reply })
            .await?
    }

    pub async fn messages_page(
        &self,
        conversation_id: Uuid,
        limit: u32,
    ) -> Result<Vec<MessageRow>> {
        self.ask(|reply| Command::MessagesPage {
            conversation_id,
            limit,
            reply,
        })
        .await?
    }

    pub async fn send_text(&self, recipient: Uuid, body: String) -> Result<()> {
        self.ask(|reply| Command::SendText {
            recipient,
            body,
            reply,
        })
        .await?
    }

    pub async fn receive_once(&self) -> Result<()> {
        self.ask(Command::ReceiveOnce).await?
    }

    pub async fn call_snapshot(&self) -> Result<CallSnapshot> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Command::CallSnapshot(reply))
            .map_err(|_| anyhow!("session actor stopped"))?;
        Ok(rx
            .await
            .map_err(|_| anyhow!("session actor dropped reply"))?)
    }

    pub async fn start_call(&self, peer: Uuid) -> Result<()> {
        self.ask(|reply| Command::StartCall { peer, reply }).await?
    }

    pub async fn hangup(&self) -> Result<()> {
        self.ask(Command::Hangup).await?
    }

    pub async fn accept(&self) -> Result<()> {
        self.ask(Command::Accept).await?
    }

    pub async fn set_muted(&self, muted: bool) -> Result<()> {
        self.ask(|reply| Command::SetMuted { muted, reply }).await?
    }
}

async fn run_actor(mut session: Session, mut rx: mpsc::UnboundedReceiver<Command>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Whoami(reply) => {
                let _ = reply.send(session.whoami_string().await);
            }
            Command::ListConversations { limit, reply } => {
                let _ = reply.send(session.app.list_conversations(limit));
            }
            Command::MessagesPage {
                conversation_id,
                limit,
                reply,
            } => {
                let _ = reply.send(session.app.messages_page(conversation_id, None, limit));
            }
            Command::SendText {
                recipient,
                body,
                reply,
            } => {
                let _ = reply.send(session.send_text(recipient, body).await);
            }
            Command::ReceiveOnce(reply) => {
                let _ = reply.send(session.receive_loop(false).await);
            }
            Command::CallSnapshot(reply) => {
                let _ = reply.send(session.calls.snapshot().await);
            }
            Command::StartCall { peer, reply } => {
                let _ = reply.send(session.start_voice_call(peer).await);
            }
            Command::Hangup(reply) => {
                let _ = reply.send(
                    session
                        .calls
                        .hangup()
                        .await
                        .map_err(|e| anyhow!("{e}")),
                );
            }
            Command::Accept(reply) => {
                let _ = reply.send(session.calls.accept().await.map_err(|e| anyhow!("{e}")));
            }
            Command::SetMuted { muted, reply } => {
                let _ = reply.send(
                    session
                        .calls
                        .set_muted(muted)
                        .await
                        .map_err(|e| anyhow!("{e}")),
                );
            }
        }
    }
}

fn fail_cmd(cmd: Command, err: anyhow::Error) {
    let msg = err.to_string();
    match cmd {
        Command::Whoami(r) => {
            let _ = r.send(Err(anyhow!(msg)));
        }
        Command::ListConversations { reply, .. } => {
            let _ = reply.send(Err(anyhow!(msg)));
        }
        Command::MessagesPage { reply, .. } => {
            let _ = reply.send(Err(anyhow!(msg)));
        }
        Command::SendText { reply, .. } => {
            let _ = reply.send(Err(anyhow!(msg)));
        }
        Command::ReceiveOnce(r) => {
            let _ = r.send(Err(anyhow!(msg)));
        }
        Command::CallSnapshot(r) => {
            let _ = r.send(CallSnapshot::idle());
        }
        Command::StartCall { reply, .. }
        | Command::Hangup(reply)
        | Command::Accept(reply)
        | Command::SetMuted { reply, .. } => {
            let _ = reply.send(Err(anyhow!(msg)));
        }
    }
}
