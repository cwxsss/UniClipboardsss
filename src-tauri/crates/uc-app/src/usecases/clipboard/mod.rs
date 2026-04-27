//! Clipboard shim modules surviving D16-1.
//!
//! `clipboard_write_coordinator` re-exports the real type from
//! `uc-application::clipboard_write`; `integration_mode` re-exports the
//! `uc-core::clipboard::ClipboardIntegrationMode` enum. Both stay here
//! only to keep `uc_app::usecases::clipboard::*` import paths working
//! for uc-bootstrap / uc-tauri until D16-2 rewires those callers.

pub mod clipboard_write_coordinator;
pub mod integration_mode;

pub use integration_mode::ClipboardIntegrationMode;
