//! FaintSignal messaging core on top of presage / libsignal.

mod actor;
mod convert;
mod session;

pub use actor::{LinkStart, SessionHandle};
pub use convert::Incoming;
pub use session::{
    linux_rss_kb, open_signal_store, print_rss, Session, SessionConfig, Servers,
};

pub use faint_signal_calls::{
    CallEvent, CallManager, CallPhase, CallSnapshot, MediaEngine, NullMediaEngine,
};
pub use faint_signal_store::{paths, AppStore, ConversationRow, FaintPaths, MessageRow};
