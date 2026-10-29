//! System tray via StatusNotifierItem (ksni).
//!
//! Left-click toggles the main window. Right-click opens Show / Settings / Quit.

use ksni::blocking::TrayMethods;
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip, Tray};
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Toggle,
    Show,
    OpenSettings,
    Quit,
}

/// Running tray icon plus the action channel iced polls.
pub struct TrayHost {
    rx: Receiver<TrayAction>,
    _handle: ksni::blocking::Handle<FaintTray>,
}

impl TrayHost {
    pub fn poll(&self) -> Vec<TrayAction> {
        let mut out = Vec::new();
        while let Ok(action) = self.rx.try_recv() {
            out.push(action);
        }
        out
    }
}

struct FaintTray {
    tx: Sender<TrayAction>,
    icon: Icon,
}

impl FaintTray {
    fn menu_item(label: &str, action: TrayAction, tx: &Sender<TrayAction>) -> MenuItem<Self> {
        let tx = tx.clone();
        StandardItem {
            label: label.into(),
            activate: Box::new(move |_| {
                let _ = tx.send(action);
            }),
            ..Default::default()
        }
        .into()
    }
}

impl Tray for FaintTray {
    fn id(&self) -> String {
        "faintsignal".into()
    }

    fn title(&self) -> String {
        "FaintSignal".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![self.icon.clone()]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "FaintSignal".into(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayAction::Toggle);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            Self::menu_item("Show", TrayAction::Show, &self.tx),
            Self::menu_item("Settings", TrayAction::OpenSettings, &self.tx),
            Self::menu_item("Quit", TrayAction::Quit, &self.tx),
        ]
    }
}

/// Start the status-area icon. Returns `None` when SNI or the icon fails.
pub fn create() -> Option<TrayHost> {
    let (tx, rx) = mpsc::channel();
    let Some(icon) = load_icon() else {
        tracing::warn!("tray icon decode failed");
        return None;
    };
    match (FaintTray { tx, icon }).spawn() {
        Ok(handle) => Some(TrayHost {
            rx,
            _handle: handle,
        }),
        Err(e) => {
            tracing::warn!("tray SNI spawn failed: {e}");
            None
        }
    }
}

fn load_icon() -> Option<Icon> {
    let bytes = crate::APP_ICON_PNG;
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let resized = image::imageops::resize(&img, 64, 64, image::imageops::FilterType::Triangle);
    let (w, h) = resized.dimensions();
    let mut data = resized.into_raw();
    // ksni expects ARGB32; image gives RGBA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
    Some(Icon {
        width: w as i32,
        height: h as i32,
        data,
    })
}
