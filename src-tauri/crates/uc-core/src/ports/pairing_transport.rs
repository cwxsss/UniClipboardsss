//! Pairing transport port (legacy, libp2p-era).
//!
//! Defines session-oriented transport capabilities used by the original
//! libp2p pairing workflow. Kept alive only while the libp2p adapter is
//! frozen (D1). New code must use
//! [`PairingSessionPort`](crate::ports::pairing::PairingSessionPort) + the
//! companion [`PairingEventPort`](crate::ports::pairing::PairingEventPort)
//! which have no `peer_id: String` leakage.

use crate::network::PairingMessage;
use anyhow::Result;
use async_trait::async_trait;

#[deprecated(
    since = "slice-1",
    note = "Use `PairingSessionPort` + `PairingEventPort` (uc-core/ports/pairing). \
            Scheduled for removal in Slice 5 with the libp2p adapter."
)]
#[async_trait]
pub trait PairingTransportPort: Send + Sync {
    /// Open a pairing session-specific stream toward a peer. Best-effort.
    async fn open_pairing_session(&self, peer_id: String, session_id: String) -> Result<()>;

    /// Send a message on an already opened pairing session stream.
    async fn send_pairing_on_session(&self, message: PairingMessage) -> Result<()>;

    /// Close a pairing session stream, optionally reporting a reason.
    async fn close_pairing_session(&self, session_id: String, reason: Option<String>)
        -> Result<()>;

    /// Unpair a device.
    async fn unpair_device(&self, peer_id: String) -> Result<()>;
}
