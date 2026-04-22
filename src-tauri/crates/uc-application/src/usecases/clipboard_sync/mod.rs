//! Slice 2 Phase 2 — clipboard sync use cases.
//!
//! * [`DispatchClipboardEntryUseCase`] — encrypt + fan-out a freshly
//!   captured clipboard entry to every reachable member.
//! * [`IngestInboundClipboardUseCase`] — subscribe to the receiver port,
//!   decrypt + re-broadcast each inbound payload as an application-level
//!   notice.
//!
//! Both are `pub(crate)` per `uc-application/AGENTS.md` §11.4. External
//! consumers (daemon / Tauri / CLI) reach them through
//! `ClipboardSyncFacade`.

pub(crate) mod dispatch_entry;
pub(crate) mod ingest_inbound;
pub(crate) mod payload_codec;

pub(crate) use dispatch_entry::{
    DispatchClipboardEntryInput, DispatchClipboardEntryUseCase, DispatchOutcome, DispatchPerTarget,
    DispatchSyncError,
};
pub(crate) use ingest_inbound::{
    InboundAction, InboundClipboardNotice, IngestInboundClipboardUseCase, IngestSpawnHandle,
};
pub(crate) use payload_codec::{decode_v3_bytes_to_snapshot, encode_snapshot_to_v3_bytes};
