//! Setup application module.
//!
//! Per `uc-core/AGENTS.md` §9.1 the setup flow is an application concern, not
//! a core one — so the state / event / action / state-machine / error types
//! and the orchestrator + action executor all live here in `uc-application`.
//! Only the persistable `SetupStatus` (at `uc-core::setup::status`) stays in
//! core since it is the data contract for `SetupStatusPort`.
//!
//! Per §11.4 the orchestrator/action-executor are internal coordination types
//! and will be made `pub(crate)` in phase B.3/B.4 once `SetupFacade` lands.
//! They are currently `pub` to keep bootstrap/daemon wiring compiling during
//! the incremental migration.

pub mod action_executor;
pub mod actions;
pub mod context;
pub mod errors;
pub mod event_port;
pub mod events;
pub mod mark_complete;
pub mod orchestrator;
pub mod pairing_facade;
pub mod ports;
pub mod state;
pub mod state_machine;

pub use actions::SetupAction;
pub use errors::SetupError;
pub use event_port::SetupEventPort;
pub use events::SetupEvent;
pub use mark_complete::MarkSetupComplete;
pub use orchestrator::{SetupError as SetupOrchestratorError, SetupOrchestrator};
pub use pairing_facade::SetupPairingFacadePort;
pub use ports::{SetupAppLifecyclePort, SetupInitializeEncryptionPort};
pub use state::SetupState;
pub use state_machine::SetupStateMachine;
