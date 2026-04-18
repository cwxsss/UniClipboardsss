pub mod crypto;
pub mod events;
pub mod facade;
pub mod orchestrator;
mod protocol_handler;
pub(crate) mod session_manager;
pub mod staged_paired_device_store;
pub mod state_machine;

pub use crypto::PairingCryptoPorts;
pub use events::{PairingDomainEvent, PairingEventPort};
pub use facade::PairingFacade;
pub use orchestrator::{PairingConfig, PairingOrchestrator};
pub use staged_paired_device_store::StagedPairedDeviceStore;
pub use state_machine::{
    CancellationBy, FailureReason, PairingAction, PairingEvent, PairingPolicy, PairingState,
    PairingStateMachine, TimeoutKind,
};
