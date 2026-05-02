//! `ClipboardSyncFacade` — Slice 2 Phase 2 public entry point.
//!
//! Per `uc-application/AGENTS.md` §11.4, the facade is the only type
//! external crates may hold. Internally it wraps
//! [`DispatchClipboardEntryUseCase`] and [`IngestInboundClipboardUseCase`]
//! and re-exports their public-shape types (`DispatchOutcome`,
//! `InboundClipboardNotice`, …) so CLI / daemon / Tauri never import
//! from `usecases::*` directly.

mod facade;

pub use facade::{
    ClipboardSyncDeps, ClipboardSyncError, ClipboardSyncFacade, DispatchEntryInput,
    DispatchEntryOutcome, DispatchEntryPerTarget, InboundAction, InboundNotice, IngestHandle,
};
