//! `AppFacade` — single, cross-domain entry point introduced in Slice 1.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers only see the
//! facade + its command / result / error types. Use cases live under
//! `crate::usecases::<domain>` and stay `pub(crate)`; the facade is
//! expected to compose them in P4 without re-exporting them.

pub mod app_facade;
pub mod commands;
pub mod errors;

pub use app_facade::AppFacade;
pub use commands::{
    InitializeSpaceCommand, InitializeSpaceResult, UnlockSpaceCommand, UnlockSpaceResult,
};
pub use errors::{InitializeSpaceError, UnlockSpaceError};
