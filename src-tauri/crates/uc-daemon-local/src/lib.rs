//! Local daemon runtime metadata and process coordination helpers.

pub mod auth;
#[cfg(feature = "sidecar-lifecycle")]
pub mod daemon_bootstrap;
#[cfg(feature = "sidecar-lifecycle")]
pub mod daemon_lifecycle;
pub mod process_metadata;
pub mod socket;
