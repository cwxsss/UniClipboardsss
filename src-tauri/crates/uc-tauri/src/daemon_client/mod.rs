pub mod pairing;
pub mod query;
pub mod setup;

pub use pairing::{DaemonPairingRequestError, TauriDaemonPairingClient};
pub use query::TauriDaemonQueryClient;
pub use setup::TauriDaemonSetupClient;

#[cfg(test)]
mod query_tests;
