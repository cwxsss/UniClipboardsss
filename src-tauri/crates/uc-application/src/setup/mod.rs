//! Setup application module.
//!
//! Per `uc-core/AGENTS.md` §9.1 the setup flow is an application concern, not
//! a core one — so the state / event / action / state-machine / error types
//! and the orchestrator + action executor all live here in `uc-application`.
//! Only the persistable `SetupStatus` (at `uc-core::setup::status`) stays in
//! core since it is the data contract for `SetupStatusPort`.
//!
//! Per `uc-application/AGENTS.md` §11.4 external consumers never see the
//! orchestrator / action-executor / context directly. They drive setup
//! through [`facade::SetupFacade`] only. Internal coordination types are
//! `pub(crate)`.

pub mod actions;
pub(crate) mod context;
pub mod errors;
pub mod event_port;
pub mod events;
pub mod facade;
pub mod is_complete;
pub mod mark_complete;
pub(crate) mod orchestrator;
pub mod pairing_facade;
pub mod ports;
pub mod state;
pub mod state_machine;
pub(crate) mod usecases;

pub(crate) mod action_executor;

#[cfg(test)]
pub(crate) mod testing;

pub use actions::SetupAction;
pub use errors::SetupError;
pub use event_port::SetupEventPort;
pub use events::SetupEvent;
pub use facade::SetupFacade;
pub use is_complete::IsSetupComplete;
pub use mark_complete::MarkSetupComplete;
pub use orchestrator::SetupError as SetupOrchestratorError;
pub use pairing_facade::SetupPairingFacadePort;
pub use ports::SetupAppLifecyclePort;
pub use state::SetupState;
pub use state_machine::SetupStateMachine;
