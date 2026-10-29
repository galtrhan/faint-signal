//! FaintSignal Linux UI — Signal Desktop–style chat layout (iced).

mod settings;
mod theme;
mod tray;

use faint_signal_calls::{CallPhase, CallSnapshot};
use faint_signal_core::{print_rss, SessionConfig, SessionHandle};
use faint_signal_store::{ConversationRow, MessageRow};
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, qr_code, row, scrollable, stack, text, text_input, toggler, Space,
};
use iced::widget::qr_code::Data as QrData;
use iced::window::{self, Id as WindowId};
use iced::{exit, Color, Element, Length, Padding, Size, Subscription, Task, Theme};
use settings::UiSettings;
use theme::{GRAY_02, GRAY_25, GRAY_45, GRAY_95, STAMP_OUT};
use tray::{TrayAction, TrayHost};
use uuid::Uuid;

pub(crate) const APP_ICON_PNG: &[u8] = include_bytes!("../assets/icons/faintsignal-256.png");

fn main() -> iced::Result {
    init_logging();
    print_rss("ui-start");
    iced::daemon(App::title, App::update, App::view)
        .theme(|_app, _id| Theme::Dark)
        .subscription(App::subscription)
        .run_with(App::new)
}

/// Release builds stay quiet unless `RUST_LOG` is set. Debug always logs.
fn init_logging() {
    let force = cfg!(debug_assertions) || std::env::var_os("RUST_LOG").is_some();
    if !force {
        return;
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("faint_signal=info".parse().unwrap()),
        )
        .try_init();
}

fn app_icon() -> Option<window::Icon> {
    window::icon::from_file_data(APP_ICON_PNG, None).ok()
}

fn focus_window(id: WindowId) -> Task<Message> {
    Task::batch([
        window::change_mode(id, window::Mode::Windowed),
        window::gain_focus(id),
    ])
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
    /// QR payload while the link overlay is open.
    link_qr: Option<QrData>,
    /// Abort handle for the whoami wait task.
    link_abort: Option<iced::task::Handle>,
    /// Cancels the link actor before the phone finishes scanning.
    link_cancel: Option<tokio::sync::oneshot::Sender<()>>,
    /// True while a receive+refresh task is in flight (skip stacked 5s ticks).
    syncing: bool,
    prefs: UiSettings,
    main_window: WindowId,
    settings_window: Option<WindowId>,
    /// False while the main window is hidden to the tray.
    main_visible: bool,
    /// Kept alive while "Use system Tray" is on.
    tray: Option<TrayHost>,
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
    LinkQr(Result<String, String>),
    LinkDone(Result<(SessionHandle, String), String>),
    CancelLink,
    CallPeer,
    Hangup,
    AcceptCall,
    MuteToggle,
    OpenSettings,
    CloseSettings,
    WindowCloseRequested(WindowId),
    WindowClosed(WindowId),
    ToggleUseSystemTray(bool),
    ToggleCloseToSystemTray(bool),
    PollTray,
    Tray(TrayAction),
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
        let prefs = UiSettings::load();
        let (main_window, open_main) = window::open(window::Settings {
            icon: app_icon(),
            size: Size::new(1100.0, 720.0),
            exit_on_close_request: false,
            ..Default::default()
        });
        let tray = if prefs.use_system_tray {
            tray::create()
        } else {
            None
        };
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
                link_qr: None,
                link_abort: None,
                link_cancel: None,
                syncing: false,
                prefs,
                main_window,
                settings_window: None,
                main_visible: true,
                tray,
            },
            Task::batch([
                open_main.discard(),
                Task::perform(boot_existing(), Message::Boot),
            ]),
        )
    }

    fn title(&self, id: WindowId) -> String {
        if self.settings_window == Some(id) {
            "Settings".into()
        } else {
            "FaintSignal".into()
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            window::close_requests().map(Message::WindowCloseRequested),
            window::close_events().map(Message::WindowClosed),
        ];
        if self.session.is_some() {
            subs.push(
                iced::time::every(std::time::Duration::from_secs(5)).map(|_| Message::Refresh),
            );
        }
        if self.tray.is_some() {
            subs.push(
                iced::time::every(std::time::Duration::from_millis(400)).map(|_| Message::PollTray),
            );
        }
        Subscription::batch(subs)
    }

    fn sync_tray(&mut self) {
        match (self.prefs.use_system_tray, self.tray.is_some()) {
            (true, false) => self.tray = tray::create(),
            (false, true) => self.tray = None,
            _ => {}
        }
    }

    fn open_settings_window(&mut self) -> Task<Message> {
        if let Some(id) = self.settings_window {
            return focus_window(id);
        }
        let (id, open) = window::open(window::Settings {
            icon: app_icon(),
            size: Size::new(420.0, 260.0),
            min_size: Some(Size::new(360.0, 220.0)),
            resizable: false,
            exit_on_close_request: false,
            ..Default::default()
        });
        self.settings_window = Some(id);
        open.discard()
    }

    fn show_main_window(&mut self) -> Task<Message> {
        self.main_visible = true;
        focus_window(self.main_window)
    }

    /// Hide the main window and close Settings if it is open.
    fn hide_to_tray(&mut self) -> Task<Message> {
        self.main_visible = false;
        let mut tasks = vec![window::change_mode(
            self.main_window,
            window::Mode::Hidden,
        )];
        if let Some(id) = self.settings_window {
            tasks.push(window::close(id));
        }
        Task::batch(tasks)
    }

    fn can_hide_to_tray(&self) -> bool {
        self.prefs.should_close_to_tray() && self.tray.is_some()
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
                let start = match SessionHandle::link_device(SessionConfig::default()) {
                    Ok(s) => s,
                    Err(e) => {
                        self.status = format!("Link failed: {e}");
                        return Task::none();
                    }
                };
                self.linking = true;
                self.link_qr = None;
                self.link_cancel = Some(start.cancel);
                self.status = "Preparing link…".into();
                let handle = start.handle;
                let url_rx = start.provisioning_url;
                let done_rx = start.done;
                let wait_url = Task::perform(
                    async move {
                        url_rx
                            .await
                            .map_err(|_| "link cancelled before QR".into())
                    },
                    Message::LinkQr,
                );
                let (wait_done, abort) =
                    Task::perform(wait_link(handle, done_rx), Message::LinkDone).abortable();
                self.link_abort = Some(abort);
                return Task::batch([wait_url, wait_done]);
            }
            Message::LinkQr(Ok(url)) => {
                if !self.linking {
                    return Task::none();
                }
                match QrData::new(url.as_bytes()) {
                    Ok(data) => {
                        self.link_qr = Some(data);
                        self.status = "Scan the QR code with Signal on your phone".into();
                    }
                    Err(e) => {
                        self.clear_link_ui();
                        self.status = format!("QR encode failed: {e}");
                    }
                }
            }
            Message::LinkQr(Err(e)) => {
                if self.linking {
                    self.clear_link_ui();
                    self.status = e;
                }
            }
            Message::LinkDone(Ok((handle, who))) => {
                if !self.linking {
                    return Task::none();
                }
                self.clear_link_ui();
                self.session = Some(handle.clone());
                self.syncing = true;
                self.status = format!("Syncing inbox… ({who})");
                return Task::perform(
                    sync_then(handle, None, true, true),
                    Message::Refreshed,
                );
            }
            Message::LinkDone(Err(e)) => {
                if !self.linking {
                    return Task::none();
                }
                self.clear_link_ui();
                self.status = if e.contains("cancelled") {
                    "Link cancelled".into()
                } else {
                    format!("Link failed: {e}")
                };
            }
            Message::CancelLink => {
                if !self.linking {
                    return Task::none();
                }
                self.clear_link_ui();
                self.status = "Link cancelled".into();
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
            Message::OpenSettings => return self.open_settings_window(),
            Message::CloseSettings => {
                if let Some(id) = self.settings_window {
                    return window::close(id);
                }
            }
            Message::WindowCloseRequested(id) => {
                if Some(id) == self.settings_window {
                    return window::close(id);
                }
                if id == self.main_window {
                    if self.can_hide_to_tray() {
                        return self.hide_to_tray();
                    }
                    return exit();
                }
            }
            Message::WindowClosed(id) => {
                if Some(id) == self.settings_window {
                    self.settings_window = None;
                }
                if id == self.main_window && !self.can_hide_to_tray() {
                    return exit();
                }
            }
            Message::ToggleUseSystemTray(on) => {
                self.prefs.use_system_tray = on;
                if !on {
                    self.prefs.close_to_system_tray = false;
                }
                self.prefs.save();
                self.sync_tray();
            }
            Message::ToggleCloseToSystemTray(on) => {
                if !self.prefs.use_system_tray {
                    return Task::none();
                }
                self.prefs.close_to_system_tray = on;
                self.prefs.save();
            }
            Message::PollTray => {
                let Some(host) = self.tray.as_ref() else {
                    return Task::none();
                };
                let actions = host.poll();
                if actions.is_empty() {
                    return Task::none();
                }
                return Task::batch(actions.into_iter().map(|a| Task::done(Message::Tray(a))));
            }
            Message::Tray(action) => match action {
                TrayAction::Toggle => {
                    if self.main_visible {
                        return self.hide_to_tray();
                    }
                    return self.show_main_window();
                }
                TrayAction::Show => return self.show_main_window(),
                TrayAction::OpenSettings => {
                    let show = self.show_main_window();
                    let settings = self.open_settings_window();
                    return Task::batch([show, settings]);
                }
                TrayAction::Quit => return exit(),
            },
        }
        Task::none()
    }

    fn view(&self, id: WindowId) -> Element<'_, Message> {
        if self.settings_window == Some(id) {
            return settings_view(self);
        }
        main_view(self)
    }

    fn selected_conversation(&self) -> Option<&ConversationRow> {
        let id = self.selected?;
        self.conversations.iter().find(|c| c.id == id)
    }

    fn clear_link_ui(&mut self) {
        if let Some(abort) = self.link_abort.take() {
            abort.abort();
        }
        if let Some(cancel) = self.link_cancel.take() {
            let _ = cancel.send(());
        }
        self.linking = false;
        self.link_qr = None;
    }
}

fn main_view(app: &App) -> Element<'_, Message> {
    let body = row![
        sidebar(app),
        container(Space::new(1, Length::Fill)).style(theme::divider),
        main_pane(app),
    ]
    .height(Length::Fill);

    let base = container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| theme::solid(GRAY_95));

    if app.linking {
        stack![base, link_overlay(app)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        base.into()
    }
}

fn link_overlay(app: &App) -> Element<'_, Message> {
    let qr: Element<'_, Message> = match &app.link_qr {
        Some(data) => container(qr_code(data).cell_size(6))
            .padding(12)
            .style(|_| theme::solid(Color::WHITE))
            .into(),
        None => text("Preparing QR code…")
            .size(14)
            .color(GRAY_45)
            .into(),
    };

    let mut steps = column![
        text(
            "On your phone open Signal, then open Settings, Linked devices, and Link new device."
        )
        .size(14)
        .color(GRAY_25),
    ]
    .spacing(8)
    .align_x(Horizontal::Center);

    if app.link_qr.is_some() {
        steps = steps.push(
            text("Scan this QR code with your phone.")
                .size(14)
                .color(GRAY_25),
        );
    }

    let panel = container(
        column![
            row![
                text("Link device").size(20).color(GRAY_02),
                Space::with_width(Length::Fill),
                button(text("×").size(18))
                    .padding([4.0, 10.0])
                    .style(theme::ghost_btn)
                    .on_press(Message::CancelLink),
            ]
            .align_y(Vertical::Center),
            steps,
            Space::with_height(8),
            qr,
            Space::with_height(12),
            button(text("Cancel linking").size(13))
                .padding([10.0, 16.0])
                .style(theme::danger_btn)
                .on_press(Message::CancelLink),
        ]
        .spacing(10)
        .align_x(Horizontal::Center)
        .max_width(420),
    )
    .padding(28)
    .style(theme::link_panel);

    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::link_dim)
        .into()
}

fn settings_view(app: &App) -> Element<'_, Message> {
    let use_tray = toggler(app.prefs.use_system_tray)
        .label("Use system Tray")
        .on_toggle(Message::ToggleUseSystemTray);

    let mut close_to_tray =
        toggler(app.prefs.close_to_system_tray).label("Close to System Tray");
    if app.prefs.use_system_tray {
        close_to_tray = close_to_tray.on_toggle(Message::ToggleCloseToSystemTray);
    }

    let mut body = column![
        text("Settings").size(22).color(GRAY_02),
        text("Tray").size(14).color(GRAY_45),
        use_tray,
        close_to_tray,
    ]
    .spacing(16);

    if !app.prefs.use_system_tray {
        body = body.push(
            text("Turn on Use system Tray first.")
                .size(12)
                .color(GRAY_45),
        );
    }

    body = body.push(Space::with_height(Length::Fill)).push(
        button(text("Close").size(13))
            .padding([8.0, 16.0])
            .style(theme::ghost_btn)
            .on_press(Message::CloseSettings),
    );

    container(body.padding(24).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| theme::solid(GRAY_95))
        .into()
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
            button(text("⚙").size(14))
                .padding([6.0, 10.0])
                .style(theme::ghost_btn)
                .on_press(Message::OpenSettings),
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

async fn wait_link(
    handle: SessionHandle,
    done: tokio::sync::oneshot::Receiver<Result<String, String>>,
) -> Result<(SessionHandle, String), String> {
    match done.await {
        Ok(Ok(whoami)) => Ok((handle, whoami)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("link cancelled".into()),
    }
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
