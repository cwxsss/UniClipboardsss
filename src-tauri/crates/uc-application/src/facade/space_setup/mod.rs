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
mod facade;

pub use commands::{
    InitializeSpaceCommand, InitializeSpaceResult, UnlockSpaceCommand, UnlockSpaceResult,
};
pub use deps::SpaceSetupDeps;
pub use errors::{InitializeSpaceError, UnlockSpaceError};
pub use facade::SpaceSetupFacade;
