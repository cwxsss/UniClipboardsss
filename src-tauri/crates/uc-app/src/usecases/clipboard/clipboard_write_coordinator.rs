//! Deprecated — moved to `uc_application::clipboard_write`.
//!
//! This module is a thin re-export shim. Slice 2 Phase 3 (T0b) migrated
//! the real `ClipboardWriteCoordinator` + `ClipboardWriteIntent` source
//! to `uc-application`; remaining consumers (daemon workers, tauri runtime,
//! uc-app's own `restore_clipboard_selection`) continue to import from
//! this path unchanged. Slice 5 / `uc-app` retirement deletes this shim
//! and rewrites callers to import from `uc_application::clipboard_write`
//! directly.
//!
//! Reason for migration: `uc-application::ApplyInboundClipboardUseCase`
//! (Slice 2 Phase 3 · T4) depends on `ClipboardWriteCoordinator` to write
//! inbound content to the OS clipboard with the correct `RemotePush`
//! intent guard; a reverse `uc-application → uc-app` import would violate
//! `uc-app/AGENTS.md` §3 dependency direction.

pub use uc_application::clipboard_write::{ClipboardWriteCoordinator, ClipboardWriteIntent};
