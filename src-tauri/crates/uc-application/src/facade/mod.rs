//! `AppFacade` — single, cross-domain entry point introduced in Slice 1.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers only see the
//! facade + its command / result / error types. The use cases and other
//! coordination objects stay `pub(crate)`.

pub mod commands;
pub mod errors;
pub(crate) mod usecases;

pub use commands::{
    InitializeSpaceCommand, InitializeSpaceResult, UnlockSpaceCommand, UnlockSpaceResult,
};
pub use errors::{InitializeSpaceError, UnlockSpaceError};
