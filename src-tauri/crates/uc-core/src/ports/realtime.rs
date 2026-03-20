use async_trait::async_trait;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RealtimeTopic {
    Pairing,
    Peers,
    PairedDevices,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceholderPayload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeEvent {
    PairingUpdated(PlaceholderPayload),
    PairingVerificationRequired(PlaceholderPayload),
    PairingFailed(PlaceholderPayload),
    PairingComplete(PlaceholderPayload),
    PeersChanged(PlaceholderPayload),
    PeersNameUpdated(PlaceholderPayload),
    PeersConnectionChanged(PlaceholderPayload),
    PairedDevicesChanged(PlaceholderPayload),
}

pub const FRONTEND_REALTIME_EVENT: &str = "daemon://stub";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealtimeFrontendPayload {
    Placeholder(PlaceholderPayload),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealtimeFrontendEvent {
    pub topic: RealtimeTopic,
    pub event_type: &'static str,
    pub ts: i64,
    pub payload: RealtimeFrontendPayload,
}

impl RealtimeFrontendEvent {
    pub fn new(
        topic: RealtimeTopic,
        event_type: &'static str,
        ts: i64,
        payload: RealtimeFrontendPayload,
    ) -> Self {
        Self {
            topic,
            event_type,
            ts,
            payload,
        }
    }

    pub fn event_type(&self) -> &'static str {
        self.event_type
    }
}

#[async_trait]
pub trait RealtimeTopicPort: Send + Sync {
    async fn subscribe(
        &self,
        consumer: &'static str,
        topics: &[RealtimeTopic],
    ) -> anyhow::Result<mpsc::Receiver<RealtimeEvent>>;
}
