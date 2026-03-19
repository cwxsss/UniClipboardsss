use std::sync::Arc;

use tokio::sync::RwLock;

use crate::state::{DaemonPairingSessionSnapshot, RuntimeState};

pub async fn upsert_pairing_snapshot(
    state: &Arc<RwLock<RuntimeState>>,
    session_id: impl Into<String>,
    peer_id: Option<String>,
    device_name: Option<String>,
    lifecycle_state: impl Into<String>,
    updated_at_ms: i64,
) {
    let snapshot = DaemonPairingSessionSnapshot {
        session_id: session_id.into(),
        peer_id,
        device_name,
        state: lifecycle_state.into(),
        updated_at_ms,
    };

    state.write().await.upsert_pairing_session(snapshot);
}

pub async fn mark_pairing_session_terminal(
    state: &Arc<RwLock<RuntimeState>>,
    session_id: impl Into<String>,
    peer_id: Option<String>,
    device_name: Option<String>,
    lifecycle_state: impl Into<String>,
    updated_at_ms: i64,
) {
    upsert_pairing_snapshot(
        state,
        session_id,
        peer_id,
        device_name,
        lifecycle_state,
        updated_at_ms,
    )
    .await;
}

pub async fn remove_expired_pairing_session(
    state: &Arc<RwLock<RuntimeState>>,
    session_id: &str,
) -> bool {
    state
        .write()
        .await
        .remove_pairing_session(session_id)
        .is_some()
}
