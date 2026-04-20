//! Cross-domain application use cases that the future `AppFacade` (P4)
//! composes into user-facing actions.
//!
//! Modules are organised by **primary semantic domain**, not by the ports
//! each use case happens to touch — a single use case (e.g. A1
//! `InitializeSpaceUseCase`) can drive many ports, but conceptually belongs
//! to one domain (setup). Per `uc-application/AGENTS.md` §11.4 every type
//! here stays `pub(crate)`: external crates reach them exclusively through
//! `AppFacade`.

pub(crate) mod setup;
