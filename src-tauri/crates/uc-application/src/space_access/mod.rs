mod context;
mod events;
mod executor;
mod facade;
mod orchestrator;
mod persistence_adapter;
mod proof_adapter;

pub use context::{SpaceAccessContext, SpaceAccessJoinerOffer};
pub use events::{SpaceAccessCompletedEvent, SpaceAccessEventPort};
pub use executor::SpaceAccessExecutor;
pub use facade::SpaceAccessFacade;
pub use orchestrator::SpaceAccessError;
// `SpaceAccessOrchestrator` stays module-private on purpose (§11.4): every
// external caller goes through `SpaceAccessFacade`.
pub use persistence_adapter::SpaceAccessPersistenceAdapter;
pub use proof_adapter::HmacProofAdapter;
