//! Setup state machine + application-layer event port.
//!
//! Per `uc-core/AGENTS.md` §9.1 the setup flow's state machine is an
//! application concern, not a core one — so the state / event / action /
//! state-machine / error types live here in `uc-application`. Only the
//! persistable `SetupStatus` (at `uc-core::setup::status`) stays in core
//! since it is the data contract for `SetupStatusPort`.

pub mod actions;
pub mod errors;
pub mod event_port;
pub mod events;
pub mod state;
pub mod state_machine;

pub use actions::SetupAction;
pub use errors::SetupError;
pub use event_port::SetupEventPort;
pub use events::SetupEvent;
pub use state::SetupState;
pub use state_machine::SetupStateMachine;
