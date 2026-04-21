//! `SpaceSetupFacade` — lifecycle of the local encrypted space.
//!
//! Covers first-run initialization (A1 `InitializeSpaceUseCase`) and
//! post-setup unlock (A2 `UnlockSpaceUseCase`). Constructed from
//! [`SpaceSetupDeps`] so external callers (bootstrap) bundle ports into
//! one struct instead of passing a dozen positional arguments.
//!
//! Distinct from the older `crate::setup::SetupFacade`, which orchestrates
//! the device-onboarding (pairing / join) flow that predates Slice 1. The
//! two facades will co-exist until later slices consolidate them.

mod commands;
mod deps;
mod errors;
mod events;
mod facade;

pub use commands::{
    InitializeSpaceCommand, InitializeSpaceResult, IssuePairingInvitationResult,
    RedeemPairingInvitationCommand, RedeemPairingInvitationResult, UnlockSpaceCommand,
    UnlockSpaceResult,
};
pub use deps::SpaceSetupDeps;
pub use errors::{
    InitializeSpaceError, IssuePairingInvitationError, RedeemPairingInvitationError,
    TryResumeSessionError, UnlockSpaceError,
};
pub use events::PairingOutcome;
pub use facade::SpaceSetupFacade;

// T10:CLI `members` 入口需要 report / error 类型才能展示 probe 摘要;
// usecase 本身保持 `pub(crate)`(§11.4),此处只透出两个值对象。
pub use crate::usecases::presence::ensure_reachable_all::{
    EnsureReachableAllError, EnsureReachableAllReport,
};
