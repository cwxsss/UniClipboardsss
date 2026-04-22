//! Slice 1 application facade tree.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers only see the
//! top-level `AppFacade` and the per-domain sub-facades it aggregates.
//! Use cases live under `crate::usecases::<domain>` and stay `pub(crate)`;
//! sub-facades expose them through domain-scoped methods.

pub mod app_facade;
pub mod clipboard;
pub mod roster;
pub mod space_setup;

pub use app_facade::AppFacade;
pub use clipboard::{
    ClipboardSyncDeps, ClipboardSyncError, ClipboardSyncFacade, DispatchEntryInput,
    DispatchEntryOutcome, DispatchEntryPerTarget, InboundAction, InboundNotice, IngestHandle,
};
pub use roster::{MemberRosterDeps, MemberRosterFacade, PresenceEvent, RosterEntry, RosterError};
pub use space_setup::{
    InitializeSpaceCommand, InitializeSpaceError, InitializeSpaceResult, SpaceSetupDeps,
    SpaceSetupFacade, UnlockSpaceCommand, UnlockSpaceError, UnlockSpaceResult,
};
