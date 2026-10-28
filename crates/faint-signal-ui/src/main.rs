//! FaintSignal Linux UI — conversation list, thread, composer, call chrome.

use faint_signal_calls::{CallPhase, CallSnapshot};
use faint_signal_core::{print_rss, SessionConfig, SessionHandle};
use faint_signal_store::{ConversationRow, MessageRow};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Element, Length, Subscription, Task, Theme};
use uuid::Uuid;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("faint_signal=info".parse().unwrap()),
        )
        .init();
    print_rss("ui-start");
    iced::application("FaintSignal", App::update, App::view)
        .theme(|_| Theme::Dark)
        .subscription(App::subscription)
        .run_with(App::new)
}

struct App {
    status: String,
    conversations: Vec<ConversationRow>,
    messages: Vec<MessageRow>,
    selected: Option<Uuid>,
    composer: String,
    call: CallSnapshot,
    session: Option<SessionHandle>,
    linking: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Boot(Result<BootData, String>),
    Select(Uuid),
    MessagesLoaded(Result<(Uuid, Vec<MessageRow>), String>),
    ComposerChanged(String),
    Send,
    Sent(Result<(), String>),
    Refresh,
    Refreshed(Result<BootData, String>),
    LinkDevice,
    LinkDone(Result<(SessionHandle, String), String>),
    CallPeer,
    Hangup,
    AcceptCall,
    MuteToggle,
}

#[derive(Debug, Clone)]
struct BootData {
    session: Option<SessionHandle>,
    conversations: Vec<ConversationRow>,
    messages: Vec<MessageRow>,
    selected: Option<Uuid>,
    whoami: String,
    call: CallSnapshot,
    /// When false, keep the existing call chrome (message-list loads).
    update_call: bool,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        (
            Self {
                status: "Loading…".into(),
                conversations: Vec::new(),
                messages: Vec::new(),
                selected: None,
                composer: String::new(),
                call: CallSnapshot::idle(),
                session: None,
                linking: false,
            },
            Task::perform(boot_existing(), Message::Boot),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.session.is_some() {
            // Lists only; whoami is cached on Boot / LinkDone.
            iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::Refresh)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Boot(Ok(data)) | Message::Refreshed(Ok(data)) => {
                if let Some(s) = data.session {
                    self.session = Some(s);
                }
                if !data.conversations.is_empty() || self.conversations.is_empty() {
                    self.conversations = data.conversations;
                }
                if !data.whoami.is_empty() {
                    self.status = data.whoami;
                }
                if data.selected.is_some() {
                    self.selected = data.selected;
                    self.messages = data.messages;
                }
                if data.update_call {
                    self.call = data.call;
                }
            }
            Message::Boot(Err(e)) => {
                self.status = format!("Not linked ({e}). Use Link device.");
            }
            Message::Refreshed(Err(e)) => {
                self.status = format!("Refresh error: {e}");
            }
            Message::MessagesLoaded(Ok((id, messages))) => {
                self.selected = Some(id);
                self.messages = messages;
            }
            Message::MessagesLoaded(Err(e)) => {
                self.status = format!("Load messages failed: {e}");
            }
            Message::Select(id) => {
                self.selected = Some(id);
                if let Some(session) = self.session.clone() {
                    return Task::perform(load_messages(session, id), Message::MessagesLoaded);
                }
            }
            Message::ComposerChanged(s) => self.composer = s,
            Message::Send => {
                let Some(peer) = self.selected else {
                    self.status = "Select a conversation first".into();
                    return Task::none();
                };
                let body = std::mem::take(&mut self.composer);
                if body.trim().is_empty() {
                    return Task::none();
                }
                if let Some(session) = self.session.clone() {
                    return Task::perform(send_text(session, peer, body), Message::Sent);
                }
            }
            Message::Sent(Ok(())) => {
                self.status = "OK".into();
                if let Some(session) = self.session.clone() {
                    let selected = self.selected;
                    return Task::perform(refresh(session, selected, false), Message::Refreshed);
                }
            }
            Message::Sent(Err(e)) => self.status = format!("Failed: {e}"),
            Message::Refresh => {
                if let Some(session) = self.session.clone() {
                    let selected = self.selected;
                    return Task::perform(refresh(session, selected, false), Message::Refreshed);
                }
            }
            Message::LinkDevice => {
                if self.linking {
                    return Task::none();
                }
                self.linking = true;
                self.status =
                    "Linking… scan the QR printed on the terminal that started the UI".into();
                return Task::perform(link_device(), Message::LinkDone);
            }
            Message::LinkDone(Ok((handle, who))) => {
                self.linking = false;
                self.session = Some(handle.clone());
                self.status = who;
                return Task::perform(refresh(handle, None, false), Message::Refreshed);
            }
            Message::LinkDone(Err(e)) => {
                self.linking = false;
                self.status = format!("Link failed: {e}");
            }
            Message::CallPeer => {
                let Some(peer) = self.selected else {
                    self.status = "Select a DM to call".into();
                    return Task::none();
                };
                if let Some(session) = self.session.clone() {
                    return Task::perform(start_call(session, peer), Message::Sent);
                }
            }
            Message::Hangup => {
                if let Some(session) = self.session.clone() {
                    return Task::perform(hangup(session), Message::Sent);
                }
            }
            Message::AcceptCall => {
                if let Some(session) = self.session.clone() {
                    return Task::perform(accept_call(session), Message::Sent);
                }
            }
            Message::MuteToggle => {
                let muted = !self.call.muted;
                if let Some(session) = self.session.clone() {
                    return Task::perform(set_muted(session, muted), Message::Sent);
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let sidebar = column![
            text("FaintSignal").size(22),
            text(&self.status).size(12),
            row![
                button("Link device").on_press(Message::LinkDevice),
                button("Refresh").on_press(Message::Refresh),
            ]
            .spacing(8),
            scrollable(
                column(
                    self.conversations
                        .iter()
                        .map(|c| {
                            let label = format!("{} ({})", c.title, c.kind);
                            button(text(label))
                                .width(Length::Fill)
                                .on_press(Message::Select(c.id))
                                .into()
                        })
                        .collect::<Vec<_>>(),
                )
                .spacing(4),
            )
            .height(Length::Fill),
        ]
        .spacing(8)
        .width(280)
        .padding(12);

        let msgs = scrollable(
            column(
                self.messages
                    .iter()
                    .rev()
                    .map(|m| {
                        let body = m.body.clone().unwrap_or_else(|| "(no text)".into());
                        text(format!("{}: {}", m.sender, body)).size(14).into()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(6),
        )
        .height(Length::Fill);

        let composer = row![
            text_input("Message…", &self.composer)
                .on_input(Message::ComposerChanged)
                .on_submit(Message::Send)
                .width(Length::Fill),
            button("Send").on_press(Message::Send),
        ]
        .spacing(8);

        let main_pane = column![call_controls(&self.call), msgs, composer]
            .spacing(8)
            .padding(12)
            .width(Length::Fill);

        container(row![sidebar, Space::with_width(1), main_pane].height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn call_controls(call: &CallSnapshot) -> Element<'_, Message> {
    let phase = format!("{:?}", call.phase);
    match call.phase {
        CallPhase::Idle => row![
            text(format!("Call: {phase}")),
            button("Voice call").on_press(Message::CallPeer),
        ]
        .spacing(8)
        .into(),
        CallPhase::Ringing => row![
            text("Incoming voice call"),
            button("Accept").on_press(Message::AcceptCall),
            button("Decline").on_press(Message::Hangup),
        ]
        .spacing(8)
        .into(),
        _ => row![
            text(format!("In call ({phase}) muted={}", call.muted)),
            button(if call.muted { "Unmute" } else { "Mute" }).on_press(Message::MuteToggle),
            button("Hang up").on_press(Message::Hangup),
        ]
        .spacing(8)
        .into(),
    }
}

async fn boot_existing() -> Result<BootData, String> {
    let handle = SessionHandle::try_load(SessionConfig::default()).map_err(err)?;
    // Retry whoami briefly while the actor opens the store.
    let mut last = String::from("session actor not ready");
    for _ in 0..40 {
        match refresh(handle.clone(), None, true).await {
            Ok(data) => return Ok(data),
            Err(e) => {
                last = e;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
    Err(last)
}

async fn refresh(
    session: SessionHandle,
    selected: Option<Uuid>,
    with_whoami: bool,
) -> Result<BootData, String> {
    let whoami = if with_whoami {
        session.whoami().await.map_err(err)?
    } else {
        String::new()
    };
    let conversations = session.list_conversations(100).await.map_err(err)?;
    let selected = selected.or_else(|| conversations.first().map(|c| c.id));
    let messages = if let Some(id) = selected {
        session.messages_page(id, 100).await.map_err(err)?
    } else {
        Vec::new()
    };
    let call = session.call_snapshot().await.map_err(err)?;
    Ok(BootData {
        session: Some(session),
        conversations,
        messages,
        selected,
        whoami,
        call,
        update_call: true,
    })
}

async fn load_messages(
    session: SessionHandle,
    id: Uuid,
) -> Result<(Uuid, Vec<MessageRow>), String> {
    let messages = session.messages_page(id, 200).await.map_err(err)?;
    Ok((id, messages))
}

async fn send_text(session: SessionHandle, peer: Uuid, body: String) -> Result<(), String> {
    session.send_text(peer, body).await.map_err(err)
}

async fn link_device() -> Result<(SessionHandle, String), String> {
    let handle = SessionHandle::link_device(SessionConfig::default()).map_err(err)?;
    let mut last = String::from("link in progress");
    for _ in 0..600 {
        match handle.whoami().await {
            Ok(who) => return Ok((handle, who)),
            Err(e) => {
                last = e.to_string();
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    Err(format!("timed out waiting for link: {last}"))
}

async fn start_call(session: SessionHandle, peer: Uuid) -> Result<(), String> {
    session.start_call(peer).await.map_err(err)
}

async fn hangup(session: SessionHandle) -> Result<(), String> {
    session.hangup().await.map_err(err)
}

async fn accept_call(session: SessionHandle) -> Result<(), String> {
    session.accept().await.map_err(err)
}

async fn set_muted(session: SessionHandle, muted: bool) -> Result<(), String> {
    session.set_muted(muted).await.map_err(err)
}
