pub mod crypto;
pub mod events;
pub mod orchestrator;
mod protocol_handler;
pub(crate) mod session_manager;
pub mod state_machine;

pub use crypto::PairingCryptoPorts;
pub use events::{PairingDomainEvent, PairingEventPort};
pub use orchestrator::{PairingConfig, PairingOrchestrator};
pub use state_machine::{
    CancellationBy, FailureReason, PairingAction, PairingEvent, PairingPolicy, PairingState,
    PairingStateMachine, TimeoutKind,
};
