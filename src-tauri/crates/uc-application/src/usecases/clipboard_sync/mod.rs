//! Slice 2 Phase 2 — clipboard sync use cases.
//!
//! * [`DispatchClipboardEntryUseCase`] — encrypt + fan-out a freshly
//!   captured clipboard entry to every reachable member.
//! * [`IngestInboundClipboardUseCase`] — subscribe to the receiver port,
//!   decrypt + dedupe + persist each inbound payload.
//!
//! Both are `pub(crate)` per `uc-application/AGENTS.md` §11.4. External
//! consumers (daemon / Tauri / CLI) reach them through
//! `ClipboardSyncFacade`.

pub(crate) mod dispatch_entry;

pub(crate) use dispatch_entry::{
    DispatchClipboardEntryInput, DispatchClipboardEntryUseCase, DispatchOutcome, DispatchPerTarget,
    DispatchSyncError,
};

// `ingest_inbound` arrives with T8 — the mod declaration + re-export will
// be restored in that commit.
