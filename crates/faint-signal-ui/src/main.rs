//! FaintSignal Linux UI — Signal Desktop–style chat layout (iced).

mod theme;

use faint_signal_calls::{CallPhase, CallSnapshot};
use faint_signal_core::{print_rss, SessionConfig, SessionHandle};
use faint_signal_store::{ConversationRow, MessageRow};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Color, Element, Length, Padding, Subscription, Task, Theme};
use theme::{GRAY_02, GRAY_25, GRAY_45, GRAY_95, STAMP_OUT};
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
    filter: String,
    call: CallSnapshot,
    session: Option<SessionHandle>,
    linking: bool,
    /// True while a receive+refresh task is in flight (skip stacked 5s ticks).
    syncing: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Boot(Result<BootData, String>),
    Select(Uuid),
    MessagesLoaded(Result<(Uuid, Vec<MessageRow>), String>),
    ComposerChanged(String),
    FilterChanged(String),
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
    /// `Some` replaces the open thread. `None` keeps the current messages.
    selected: Option<Uuid>,
    whoami: String,
    call: CallSnapshot,
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
                filter: String::new(),
                call: CallSnapshot::idle(),
                session: None,
                linking: false,
                syncing: false,
            },
            Task::perform(boot_existing(), Message::Boot),
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.session.is_some() {
            iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::Refresh)
        } else {
            Subscription::none()
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Boot(Ok(data)) | Message::Refreshed(Ok(data)) => {
                self.syncing = false;
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
                self.call = data.call;
                if let Some(id) = self.selected {
                    if !self.conversations.iter().any(|c| c.id == id) {
                        self.selected = None;
                        self.messages.clear();
                    }
                }
            }
            Message::Boot(Err(e)) => {
                self.syncing = false;
                self.status = format!("Not linked ({e}). Use Link device.");
            }
            Message::Refreshed(Err(e)) => {
                self.syncing = false;
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
            Message::FilterChanged(s) => self.filter = s,
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
                if let Some(session) = self.session.clone() {
                    let selected = self.selected;
                    return Task::perform(
                        refresh(session, selected, false, false),
                        Message::Refreshed,
                    );
                }
            }
            Message::Sent(Err(e)) => self.status = format!("Failed: {e}"),
            Message::Refresh => {
                if self.syncing {
                    return Task::none();
                }
                if let Some(session) = self.session.clone() {
                    self.syncing = true;
                    let selected = self.selected;
                    return Task::perform(
                        sync_then(session, selected, false, false),
                        Message::Refreshed,
                    );
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
                self.syncing = true;
                self.status = format!("Syncing inbox… ({who})");
                return Task::perform(
                    sync_then(handle, None, true, true),
                    Message::Refreshed,
                );
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
        let body = row![
            sidebar(self),
            container(Space::new(1, Length::Fill)).style(theme::divider),
            main_pane(self),
        ]
        .height(Length::Fill);

        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| theme::solid(GRAY_95))
            .into()
    }

    fn selected_conversation(&self) -> Option<&ConversationRow> {
        let id = self.selected?;
        self.conversations.iter().find(|c| c.id == id)
    }
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let filter = app.filter.to_lowercase();
    let rows: Vec<Element<'_, Message>> = app
        .conversations
        .iter()
        .filter(|c| filter.is_empty() || c.title.to_lowercase().contains(&filter))
        .map(|c| conversation_row(c, app.selected == Some(c.id)))
        .collect();

    let list: Element<'_, Message> = if rows.is_empty() {
        container(
            text(if app.session.is_none() {
                "Link a device to load chats"
            } else if app.filter.is_empty() {
                "No conversations yet"
            } else {
                "No matches"
            })
            .size(14)
            .color(GRAY_45),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        scrollable(column(rows).spacing(2).padding([0.0, 8.0]))
            .height(Length::Fill)
            .into()
    };

    let header = column![
        row![
            text("Chats").size(22).color(GRAY_02),
            Space::with_width(Length::Fill),
            button(text("↻").size(16))
                .padding([6.0, 10.0])
                .style(theme::ghost_btn)
                .on_press(Message::Refresh),
            button(text(if app.linking { "…" } else { "Link" }).size(13))
                .padding([6.0, 10.0])
                .style(theme::ghost_btn)
                .on_press(Message::LinkDevice),
        ]
        .align_y(Vertical::Center)
        .spacing(4),
        text(&app.status).size(11).color(GRAY_45),
        text_input("Search", &app.filter)
            .on_input(Message::FilterChanged)
            .padding([8.0, 14.0])
            .size(14)
            .style(theme::search_field),
    ]
    .spacing(10)
    .padding(pad(16.0, 16.0, 12.0, 16.0));

    container(column![header, list].height(Length::Fill))
        .width(340)
        .height(Length::Fill)
        .style(theme::sidebar)
        .into()
}

fn conversation_row(c: &ConversationRow, selected: bool) -> Element<'_, Message> {
    let time = c
        .last_message_at
        .map(format_clock)
        .unwrap_or_default();

    let meta = row![
        text(kind_label(&c.kind)).size(12).color(GRAY_45),
        Space::with_width(Length::Fill),
        text(time).size(11).color(GRAY_45),
    ];

    let title_row = row![
        text(&c.title)
            .size(15)
            .color(GRAY_02)
            .width(Length::Fill),
        unread_dot(c.unread),
    ]
    .align_y(Vertical::Center)
    .spacing(8);

    let body = row![
        avatar(&c.title, c.id.as_u128(), 40.0),
        column![title_row, meta].spacing(3).width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Vertical::Center)
    .padding([10.0, 10.0]);

    button(body)
        .width(Length::Fill)
        .padding(0)
        .style(theme::conv_row(selected))
        .on_press(Message::Select(c.id))
        .into()
}

fn main_pane(app: &App) -> Element<'_, Message> {
    match app.selected_conversation() {
        Some(conv) => {
            let header = chat_header(conv, &app.call);
            let call = call_banner(&app.call);
            let thread = message_thread(&app.messages);
            let composer = composer_bar(&app.composer);
            container(column![header, call, thread, composer].height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(theme::pane)
                .into()
        }
        None => empty_pane(app),
    }
}

fn empty_pane(app: &App) -> Element<'_, Message> {
    let hint = if app.session.is_none() {
        "Link this install as a Signal linked device to open chats."
    } else {
        "Select a chat from the left, or wait for sync."
    };
    container(
        column![
            text("FaintSignal").size(28).color(GRAY_02),
            text(hint).size(14).color(GRAY_45),
            Space::with_height(12),
            button(text(if app.linking {
                "Waiting for scan…"
            } else {
                "Link device"
            }))
            .padding([10.0, 18.0])
            .style(theme::icon_btn(true))
            .on_press(Message::LinkDevice),
        ]
        .spacing(8)
        .align_x(Horizontal::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(theme::pane)
    .into()
}

fn chat_header<'a>(conv: &'a ConversationRow, call: &'a CallSnapshot) -> Element<'a, Message> {
    let subtitle = format!("{} · end-to-end encrypted", kind_label(&conv.kind));

    let actions: Element<'_, Message> = match call.phase {
        CallPhase::Idle => button(text("Voice").size(13))
            .padding([8.0, 14.0])
            .style(theme::icon_btn(false))
            .on_press(Message::CallPeer)
            .into(),
        CallPhase::Ringing => row![
            button(text("Accept").size(13))
                .padding([8.0, 14.0])
                .style(theme::success_btn)
                .on_press(Message::AcceptCall),
            button(text("Decline").size(13))
                .padding([8.0, 14.0])
                .style(theme::danger_btn)
                .on_press(Message::Hangup),
        ]
        .spacing(8)
        .into(),
        CallPhase::Dialing | CallPhase::Connecting | CallPhase::Active | CallPhase::Ending => row![
            button(text(if call.muted { "Unmute" } else { "Mute" }).size(13))
                .padding([8.0, 14.0])
                .style(theme::icon_btn(false))
                .on_press(Message::MuteToggle),
            button(text("Hang up").size(13))
                .padding([8.0, 14.0])
                .style(theme::danger_btn)
                .on_press(Message::Hangup),
        ]
        .spacing(8)
        .into(),
    };

    container(
        row![
            avatar(&conv.title, conv.id.as_u128(), 36.0),
            column![
                text(&conv.title).size(16).color(GRAY_02),
                text(subtitle).size(12).color(GRAY_45),
            ]
            .spacing(2)
            .width(Length::Fill),
            actions,
        ]
        .spacing(12)
        .align_y(Vertical::Center)
        .padding([12.0, 20.0]),
    )
    .width(Length::Fill)
    .style(theme::pane)
    .into()
}

fn call_banner(call: &CallSnapshot) -> Element<'_, Message> {
    if matches!(call.phase, CallPhase::Idle) {
        return Space::with_height(0).into();
    }
    let label = match call.phase {
        CallPhase::Idle => "",
        CallPhase::Dialing => "Calling…",
        CallPhase::Ringing => "Incoming voice call…",
        CallPhase::Connecting => "Connecting…",
        CallPhase::Active => {
            if call.muted {
                "In call · muted"
            } else {
                "In call"
            }
        }
        CallPhase::Ending => "Ending call…",
    };
    container(text(label).size(13).color(GRAY_25))
        .width(Length::Fill)
        .padding([8.0, 20.0])
        .style(theme::call_banner)
        .into()
}

fn message_thread(messages: &[MessageRow]) -> Element<'_, Message> {
    if messages.is_empty() {
        return container(text("No messages yet").size(14).color(GRAY_45))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into();
    }

    // Store returns newest-first; show chronological top → bottom.
    let bubbles: Vec<Element<'_, Message>> = messages.iter().rev().map(message_bubble).collect();

    scrollable(
        column(bubbles)
            .spacing(6)
            .padding(pad(16.0, 24.0, 8.0, 24.0)),
    )
    .height(Length::Fill)
    .into()
}

fn message_bubble(m: &MessageRow) -> Element<'_, Message> {
    let outgoing = m.direction == "out";
    let body = m.body.as_deref().unwrap_or("(no text)");
    let time = format_clock(m.sent_at);
    let stamp = if outgoing { STAMP_OUT } else { GRAY_45 };

    let mut parts: Vec<Element<'_, Message>> = Vec::new();
    if !outgoing && !m.sender.is_empty() {
        parts.push(text(&m.sender).size(11).color(GRAY_25).into());
    }
    parts.push(text(body).size(14).color(GRAY_02).into());
    parts.push(text(time).size(10).color(stamp).into());

    let bubble = container(column(parts).spacing(4))
        .padding([8.0, 12.0])
        .max_width(420)
        .style(if outgoing {
            theme::bubble_out
        } else {
            theme::bubble_in
        });

    if outgoing {
        row![Space::with_width(Length::Fill), bubble]
            .width(Length::Fill)
            .into()
    } else {
        row![bubble, Space::with_width(Length::Fill)]
            .width(Length::Fill)
            .into()
    }
}

fn composer_bar(composer: &str) -> Element<'_, Message> {
    container(
        row![
            text_input("Send a message", composer)
                .on_input(Message::ComposerChanged)
                .on_submit(Message::Send)
                .padding([10.0, 16.0])
                .size(14)
                .width(Length::Fill)
                .style(theme::composer_field),
            button(text("Send").size(13))
                .padding([10.0, 18.0])
                .style(theme::icon_btn(true))
                .on_press(Message::Send),
        ]
        .spacing(10)
        .align_y(Vertical::Center)
        .padding(pad(12.0, 20.0, 16.0, 20.0)),
    )
    .width(Length::Fill)
    .style(theme::pane)
    .into()
}

fn unread_dot<'a>(show: bool) -> Element<'a, Message> {
    if show {
        container(Space::new(8, 8))
            .style(|_: &Theme| theme::rounded(theme::ULTRAMARINE, 4.0))
            .into()
    } else {
        Space::with_width(0).into()
    }
}

fn avatar(title: &str, seed: u128, size: f32) -> Element<'static, Message> {
    let initial = title_initial(title);
    let bg = theme::avatar_color(seed);
    container(text(initial).size((size * 0.4).round()).color(Color::WHITE))
        .width(size)
        .height(size)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_: &Theme| theme::rounded(bg, size / 2.0))
        .into()
}

fn title_initial(title: &str) -> String {
    title
        .chars()
        .next()
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into())
}

fn kind_label(kind: &str) -> &'static str {
    if kind == "group" {
        "Group"
    } else {
        "Direct message"
    }
}

/// Format Signal timestamps (unix millis) as UTC `HH:MM`.
fn format_clock(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let hours = secs.div_euclid(3600).rem_euclid(24);
    let mins = secs.div_euclid(60).rem_euclid(60);
    format!("{hours:02}:{mins:02}")
}

fn pad(top: f32, right: f32, bottom: f32, left: f32) -> Padding {
    Padding {
        top,
        right,
        bottom,
        left,
    }
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

async fn boot_existing() -> Result<BootData, String> {
    let handle = SessionHandle::try_load(SessionConfig::default()).map_err(err)?;
    // Poll a cheap readiness check while the actor opens the store.
    let mut last = String::from("session actor not ready");
    for _ in 0..40 {
        match handle.whoami().await {
            Ok(_) => return sync_then(handle, None, true, true).await,
            Err(e) => {
                last = e.to_string();
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
    Err(last)
}

async fn receive_soft(session: &SessionHandle) {
    if let Err(e) = session.receive_once().await {
        tracing::warn!(error = %e, "inbox sync failed");
    }
}

/// Soft-fail inbox pull, then read the local cache.
async fn sync_then(
    session: SessionHandle,
    selected: Option<Uuid>,
    with_whoami: bool,
    pick_default: bool,
) -> Result<BootData, String> {
    receive_soft(&session).await;
    refresh(session, selected, with_whoami, pick_default).await
}

/// Read conversations, optional open thread, and call chrome from the local cache.
/// When `pick_default` is set and nothing is selected, open the newest conversation.
async fn refresh(
    session: SessionHandle,
    selected: Option<Uuid>,
    with_whoami: bool,
    pick_default: bool,
) -> Result<BootData, String> {
    let whoami = if with_whoami {
        session.whoami().await.map_err(err)?
    } else {
        String::new()
    };
    let conversations = session.list_conversations(100).await.map_err(err)?;
    let selected = if pick_default {
        selected.or_else(|| conversations.first().map(|c| c.id))
    } else {
        selected
    };
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
