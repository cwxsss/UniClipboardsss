pub mod pairing;
pub mod query;

pub use pairing::{DaemonPairingRequestError, TauriDaemonPairingClient};
pub use query::TauriDaemonQueryClient;

#[cfg(test)]
mod query_tests;
