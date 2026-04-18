mod context;
mod crypto_adapter;
mod events;
mod executor;
mod facade;
mod initialize_new_space;
mod network_adapter;
mod orchestrator;
mod persistence_adapter;
mod proof_adapter;

pub use context::{SpaceAccessContext, SpaceAccessJoinerOffer, SpaceAccessOffer};
pub use crypto_adapter::{
    DefaultSpaceAccessCryptoFactory, SpaceAccessCryptoAdapter, SpaceAccessCryptoError,
};
pub use events::{SpaceAccessCompletedEvent, SpaceAccessEventPort};
pub use executor::SpaceAccessExecutor;
pub use facade::SpaceAccessFacade;
pub use initialize_new_space::{
    SpaceAccessCryptoFactory, StartSponsorAuthorization, StartSponsorAuthorizationError,
};
pub use network_adapter::SpaceAccessNetworkAdapter;
pub use orchestrator::SpaceAccessError;
// `SpaceAccessOrchestrator` stays module-private on purpose (§11.4): every
// external caller goes through `SpaceAccessFacade`. The in-crate consumers
// (`facade`, `initialize_new_space`) reach it via `super::orchestrator::…`.
pub use persistence_adapter::SpaceAccessPersistenceAdapter;
pub use proof_adapter::HmacProofAdapter;
