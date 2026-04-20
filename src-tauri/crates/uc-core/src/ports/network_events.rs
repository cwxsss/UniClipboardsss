//! Network event subscription port (legacy, libp2p-era).
//!
//! Carries every libp2p-origin event (`PeerDiscovered`, `PeerConnected`,
//! `PairingRequestReceived`, …). Kept alive only while the libp2p adapter
//! is frozen (D1). New Slice 1+ code subscribes to dedicated,
//! domain-scoped ports — e.g.
//! [`PairingEventPort`](crate::ports::pairing::PairingEventPort) for
//! pairing sessions — so Slice 5 can delete this whole stream in one pass.

use crate::network::NetworkEvent;
use anyhow::Result;
use async_trait::async_trait;

#[deprecated(
    since = "slice-1",
    note = "Use domain-scoped event ports (e.g. `PairingEventPort`). \
            Scheduled for removal in Slice 5 with the libp2p adapter."
)]
#[async_trait]
pub trait NetworkEventPort: Send + Sync {
    /// Subscribe to network events.
    ///
    /// Contract: adapters may expose this as a single-consumer stream.
    async fn subscribe_events(&self) -> Result<tokio::sync::mpsc::Receiver<NetworkEvent>>;
}
