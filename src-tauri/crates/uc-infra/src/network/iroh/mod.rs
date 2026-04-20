//! iroh network adapter (Slice 1+).
//!
//! Groups adapters backed by the `iroh` crate: long-term device identity,
//! endpoint lifecycle, session opener, blob transfer. Slice 1 only ships
//! [`IrohIdentityStore`]; later slices add the rest.

pub mod identity_store;
pub mod node;

pub use identity_store::{IrohIdentityStore, IDENTITY_STORE_KEY};
pub use node::{IrohNode, IrohNodeBuilder, IrohNodeConfig, IrohNodeError, PairingHandlers};
