//! Deprecated — moved to `uc_application::clipboard_capture`.
//!
//! This module is a thin re-export shim. Slice 2 Phase 3 (T0a) migrated
//! the real `CaptureClipboardUseCase` source to `uc-application`; the 18+
//! existing consumers(daemon / tauri / bootstrap / uc-app internal)
//! continue to import from this path unchanged. Slice 5 / `uc-app`
//! retirement deletes this shim and rewrites callers to import from
//! `uc_application::clipboard_capture` directly.
//!
//! Reason for migration: `uc-application::ApplyInboundClipboardUseCase`
//! (Slice 2 Phase 3 · T4) depends on `CaptureClipboardUseCase`; a reverse
//! `uc-application → uc-app` import would violate `uc-app/AGENTS.md` §3
//! dependency direction and block `uc-app` retirement.

pub use uc_application::clipboard_capture::CaptureClipboardUseCase;
