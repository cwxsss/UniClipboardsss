pub mod common;
pub mod platform;
pub mod watcher;

pub use platform::LocalClipboard;
pub use watcher::{PlatformEvent, PlatformEventSender};
