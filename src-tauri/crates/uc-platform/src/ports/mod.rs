pub mod app_dirs;
pub mod app_event_handler;
pub mod autostart;
pub mod clipboard_runtime;
pub mod command_executor;
pub mod identity_store;
pub mod observability;
pub mod ui_port;
pub mod watcher_control;

pub use app_dirs::AppDirsPort;
pub use autostart::AutostartPort;
pub use clipboard_runtime::ClipboardRuntimePort;
pub use command_executor::PlatformCommandExecutorPort;
pub use identity_store::{IdentityStoreError, IdentityStorePort};
pub use observability::{extract_trace, OptionalTrace, TraceMetadata, TraceParseError};
pub use ui_port::UiPort;
pub use watcher_control::{WatcherControlError, WatcherControlPort};
