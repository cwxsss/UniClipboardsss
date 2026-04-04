use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::security::space_access::state::SpaceAccessState;
use crate::setup::SetupState;

/// Forward-compatibility type contract for structured pairing routing metadata.
///
/// **Current status:** This struct defines the canonical observability shape for pairing
/// routing decisions but is NOT instantiated in production code paths. The live logging
/// uses [`log_bridge_routing()`](uc_daemon_client::ws_bridge::log_bridge_routing) which
/// accepts the same fields as individual `&str` parameters for zero-allocation logging.
///
/// **Why it exists:** Provides a shared, testable contract that downstream consumers
/// (future trace aggregation, structured log serialization, or Seq event enrichment)
/// can depend on without re-discovering the field set from scattered function signatures.
///
/// Used only for structured logging contracts and diagnostics — never sent over the wire or stored.
/// Must never include secrets, raw key material, fingerprints, codes, or verification payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingRoutingRecord {
    /// Session that originated the event.
    pub session_id: String,
    /// Daemon wire event type (e.g. `"pairing.verification_required"`).
    pub source_event_type: String,
    /// The `kind`/`stage` value from the payload that drove the routing decision, when present.
    pub payload_kind: Option<String>,
    /// The [`RealtimeEvent`] variant name the bridge produced (e.g. `"PairingVerificationRequired"`).
    pub routed_event_class: &'static str,
    /// Monotonic timestamp in milliseconds from the daemon event envelope (`ts` field).
    pub envelope_ts_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RealtimeTopic {
    Pairing,
    Peers,
    PairedDevices,
    Setup,
    SpaceAccess,
    Clipboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingUpdatedEvent {
    pub session_id: String,
    pub status: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingVerificationRequiredEvent {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
    pub code: Option<String>,
    pub local_fingerprint: Option<String>,
    pub peer_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingFailedEvent {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingCompleteEvent {
    pub session_id: String,
    pub peer_id: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimePeerSummary {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerChangedEvent {
    pub peers: Vec<RealtimePeerSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerNameUpdatedEvent {
    pub peer_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerConnectionChangedEvent {
    pub peer_id: String,
    pub connected: bool,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimePairedDeviceSummary {
    pub device_id: String,
    pub device_name: String,
    pub last_seen_ts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDevicesChangedEvent {
    pub devices: Vec<RealtimePairedDeviceSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetupStateChangedEvent {
    pub session_id: Option<String>,
    pub state: SetupState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupSpaceAccessCompletedEvent {
    pub session_id: String,
    pub peer_id: String,
    pub success: bool,
    pub reason: Option<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpaceAccessStateChangedEvent {
    pub state: SpaceAccessState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardNewContentEvent {
    pub entry_id: String,
    pub preview: String,
    pub origin: String, // "local" or "remote"
}

#[derive(Debug, Clone, PartialEq)]
pub enum RealtimeEvent {
    PairingUpdated(PairingUpdatedEvent),
    PairingVerificationRequired(PairingVerificationRequiredEvent),
    PairingFailed(PairingFailedEvent),
    PairingComplete(PairingCompleteEvent),
    PeersChanged(PeerChangedEvent),
    PeersNameUpdated(PeerNameUpdatedEvent),
    PeersConnectionChanged(PeerConnectionChangedEvent),
    PairedDevicesChanged(PairedDevicesChangedEvent),
    SetupStateChanged(SetupStateChangedEvent),
    SetupSpaceAccessCompleted(SetupSpaceAccessCompletedEvent),
    SpaceAccessStateChanged(SpaceAccessStateChangedEvent),
    ClipboardNewContent(ClipboardNewContentEvent),
}

#[async_trait]
pub trait RealtimeTopicPort: Send + Sync {
    async fn subscribe(
        &self,
        consumer: &'static str,
        topics: &[RealtimeTopic],
    ) -> anyhow::Result<mpsc::Receiver<RealtimeEvent>>;
}
