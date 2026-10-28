//! Linked-device session: link, send, receive, feed the app store and call manager.

use crate::convert::{content_to_incoming, Incoming};
use anyhow::{anyhow, Result};
use faint_signal_calls::{CallManager, MediaEngine, NullMediaEngine};
use faint_signal_store::{AppStore, FaintPaths};
use futures::channel::oneshot;
use futures::{future, StreamExt};
use presage::libsignal_service::configuration::SignalServers;
use presage::libsignal_service::content::{ContentBody, DataMessage};
use presage::libsignal_service::protocol::ServiceId;
use presage::libsignal_service::prelude::Uuid;
use presage::model::identity::OnNewIdentity;
use presage::model::messages::Received;
use presage::manager::Registered;
use presage::Manager;
use presage_store_sqlite::SqliteStore;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub type Servers = SignalServers;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub device_name: String,
    pub servers: SignalServers,
    pub passphrase: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            device_name: "FaintSignal".into(),
            servers: SignalServers::Production,
            passphrase: None,
        }
    }
}

pub async fn open_signal_store(
    path: impl AsRef<Path>,
    passphrase: Option<&str>,
) -> Result<SqliteStore> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path_str = path
        .as_ref()
        .to_str()
        .ok_or_else(|| anyhow!("signal db path is not utf-8"))?;
    SqliteStore::open_with_passphrase(path_str, passphrase, OnNewIdentity::Trust)
        .await
        .map_err(|e| anyhow!("open signal store: {e}"))
}

/// High-level session used by the CLI and UI.
pub struct Session {
    pub paths: FaintPaths,
    pub app: AppStore,
    pub calls: Arc<CallManager>,
    manager: Manager<SqliteStore, Registered>,
}

impl Session {
    pub async fn load(paths: FaintPaths, config: &SessionConfig) -> Result<Self> {
        paths.ensure_dirs()?;
        let app = AppStore::open(&paths.app_db)?;
        let store = open_signal_store(&paths.signal_db, config.passphrase.as_deref()).await?;
        let manager = Manager::load_registered(store)
            .await
            .map_err(|e| anyhow!("load registered manager: {e}"))?;
        Ok(Self {
            paths,
            app,
            calls: null_call_manager(),
            manager,
        })
    }

    /// Link as a secondary device. Prints a terminal QR for the phone to scan.
    pub async fn link_device(paths: FaintPaths, config: SessionConfig) -> Result<Self> {
        paths.ensure_dirs()?;
        let app = AppStore::open(&paths.app_db)?;
        let store = open_signal_store(&paths.signal_db, config.passphrase.as_deref()).await?;
        let (tx, rx) = oneshot::channel();
        let link = Manager::link_secondary_device(
            store,
            config.servers,
            config.device_name.clone(),
            tx,
        );
        let show_qr = async move {
            match rx.await {
                Ok(url) => {
                    println!("Scan this QR code with Signal on your phone:");
                    if let Err(e) = qr2term::print_qr(url.to_string()) {
                        warn!(error = %e, "qr render failed");
                        println!("Provisioning URL: {url}");
                    } else {
                        println!("Provisioning URL: {url}");
                    }
                }
                Err(e) => error!(error = %e, "linking cancelled before QR"),
            }
        };
        let (manager, _) = future::join(link, show_qr).await;
        let manager = manager.map_err(|e| anyhow!("link secondary device: {e}"))?;
        let who = manager
            .whoami()
            .await
            .map_err(|e| anyhow!("whoami after link: {e}"))?;
        info!(?who, "linked secondary device");
        Ok(Self {
            paths,
            app,
            calls: null_call_manager(),
            manager,
        })
    }

    pub async fn whoami_string(&self) -> Result<String> {
        let w = self
            .manager
            .whoami()
            .await
            .map_err(|e| anyhow!("whoami: {e}"))?;
        Ok(format!("{w:?}"))
    }

    pub async fn send_text(&mut self, recipient: Uuid, body: impl Into<String>) -> Result<()> {
        let (timestamp, msg) = text_data_message(body);
        self.manager
            .send_message(ServiceId::Aci(recipient.into()), msg, timestamp)
            .await
            .map_err(|e| anyhow!("send message: {e}"))?;
        Ok(())
    }

    /// Send text to a group identified by its 32-byte master key.
    pub async fn send_text_to_group(
        &mut self,
        master_key: &[u8; 32],
        body: impl Into<String>,
    ) -> Result<()> {
        let (timestamp, msg) = text_data_message(body);
        self.manager
            .send_message_to_group(master_key, msg, timestamp)
            .await
            .map_err(|e| anyhow!("send group message: {e}"))?;
        Ok(())
    }

    /// Download one attachment pointer to `attachments_dir` and record it.
    pub async fn save_attachment_pointer(
        &mut self,
        message_id: Uuid,
        pointer: &presage::libsignal_service::proto::AttachmentPointer,
    ) -> Result<std::path::PathBuf> {
        use faint_signal_store::AttachmentRow;
        let bytes = self
            .manager
            .get_attachment(pointer)
            .await
            .map_err(|e| anyhow!("get attachment: {e}"))?;
        let id = Uuid::new_v4();
        let content_type = pointer
            .content_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".into());
        let ext = content_type
            .split('/')
            .nth(1)
            .unwrap_or("bin")
            .replace('+', ".");
        let path = self.paths.attachments_dir.join(format!("{id}.{ext}"));
        tokio::fs::write(&path, &bytes)
            .await
            .map_err(|e| anyhow!("write attachment: {e}"))?;
        self.app.record_attachment(&AttachmentRow {
            id,
            message_id,
            path: path.display().to_string(),
            content_type,
            size_bytes: bytes.len() as i64,
        })?;
        Ok(path)
    }

    /// Receive until the first queue empty (or forever if `continuous`).
    pub async fn receive_loop(&mut self, continuous: bool) -> Result<()> {
        let mut messages = self
            .manager
            .receive_messages()
            .await
            .map_err(|e| anyhow!("receive_messages: {e}"))?;

        while let Some(received) = messages.next().await {
            match received {
                Received::QueueEmpty => {
                    info!("message queue empty (synced)");
                    if !continuous {
                        break;
                    }
                }
                Received::Contacts => {
                    info!("contacts sync received");
                }
                Received::DecryptionError(service_id) => {
                    warn!(?service_id, "decryption error");
                }
                Received::Content(content) => {
                    self.handle_content(*content).await?;
                }
                // Future protocol variants: fail closed by logging.
                #[allow(unreachable_patterns)]
                other => {
                    warn!(?other, "unsupported receive event");
                }
            }
        }
        Ok(())
    }

    async fn handle_content(
        &mut self,
        content: presage::libsignal_service::content::Content,
    ) -> Result<()> {
        match content_to_incoming(&content) {
            Incoming::Message {
                conversation,
                message,
            } => {
                self.app.upsert_inbound(&conversation, &message)?;
                if let Some(body) = &message.body {
                    println!("[{}] {}: {}", conversation.title, message.sender, body);
                }
                if let ContentBody::DataMessage(DataMessage { attachments, .. }) = &content.body {
                    for pointer in attachments {
                        match self.save_attachment_pointer(message.id, pointer).await {
                            Ok(path) => info!(path = %path.display(), "saved attachment"),
                            Err(e) => warn!(error = %e, "attachment download failed"),
                        }
                    }
                }
            }
            Incoming::Call(signal) => {
                if let Err(e) = self.calls.handle_inbound(signal).await {
                    warn!(error = %e, "call signal handling");
                }
            }
        }
        Ok(())
    }

    pub async fn start_voice_call(&self, peer: Uuid) -> Result<()> {
        self.calls
            .start_outgoing(peer)
            .await
            .map_err(|e| anyhow!("start voice call: {e}"))
    }
}

fn null_call_manager() -> Arc<CallManager> {
    Arc::new(CallManager::new(Arc::new(NullMediaEngine) as Arc<dyn MediaEngine>))
}

fn text_data_message(body: impl Into<String>) -> (u64, DataMessage) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64;
    (
        timestamp,
        DataMessage {
            body: Some(body.into()),
            timestamp: Some(timestamp),
            ..Default::default()
        },
    )
}

/// Read current process RSS in kB from `/proc/self/status` (Linux).
pub fn linux_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb);
        }
    }
    None
}

pub fn print_rss(label: &str) {
    match linux_rss_kb() {
        Some(kb) => println!("RSS {label}: {:.1} MB ({kb} kB)", kb as f64 / 1024.0),
        None => println!("RSS {label}: unavailable (non-Linux or no /proc)"),
    }
}
