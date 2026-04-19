//! Cross-domain application use cases driven by `AppFacade`.
//!
//! `AppFacade` (P4) will hold the `Arc`s of these types. They are kept
//! `pub(crate)` per `uc-application/AGENTS.md` §11.4 so external crates
//! can only drive them through the facade.

pub(crate) mod initialize_space;
pub(crate) mod unlock_space;
