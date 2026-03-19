//! WebSocket subscribe protocol for daemon read-model topics.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, warn};

use crate::api::server::DaemonApiState;
use crate::api::types::{
    DaemonWsEvent, DaemonWsSubscribeRequest, PairedDeviceDto, PairingSessionSummaryDto,
    PeerSnapshotDto, StatusResponse,
};

const TOPIC_STATUS: &str = "status";
const TOPIC_PEERS: &str = "peers";
const TOPIC_PAIRED_DEVICES: &str = "paired-devices";
const TOPIC_PAIRING: &str = "pairing";

const STATUS_SNAPSHOT_EVENT: &str = "status.snapshot";
const PEERS_SNAPSHOT_EVENT: &str = "peers.snapshot";
const PAIRED_DEVICES_SNAPSHOT_EVENT: &str = "paired-devices.snapshot";
const PAIRING_SNAPSHOT_EVENT: &str = "pairing.snapshot";

const STATUS_UPDATED_EVENT: &str = "status.updated";
const PEERS_CHANGED_EVENT: &str = "peers.changed";
const PAIRED_DEVICES_CHANGED_EVENT: &str = "paired-devices.changed";
const PAIRING_UPDATED_EVENT: &str = "pairing.updated";

type ClientTopics = Arc<RwLock<HashSet<String>>>;

pub fn router() -> Router<DaemonApiState> {
    Router::new().route("/ws", get(websocket_upgrade))
}

async fn websocket_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<DaemonApiState>,
    headers: HeaderMap,
) -> Response {
    if !state.is_authorized(&headers) {
        return unauthorized().into_response();
    }

    ws.on_upgrade(move |socket| handle_connection(socket, state))
}

async fn handle_connection(socket: WebSocket, state: DaemonApiState) {
    let topics = Arc::new(RwLock::new(HashSet::new()));
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<DaemonWsEvent>(32);
    let mut broadcast_rx = state.event_tx.subscribe();
    let fanout_topics = Arc::clone(&topics);
    let fanout_tx = outbound_tx.clone();

    let fanout_task = tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(event) => {
                    let should_deliver = {
                        let guard = fanout_topics.read().await;
                        guard.contains(event.topic.as_str())
                    };

                    if should_deliver && fanout_tx.send(event).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    warn!(
                        skipped,
                        "websocket client lagged behind daemon event stream"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let (mut sender, mut receiver) = socket.split();
    let send_task = tokio::spawn(async move {
        while let Some(event) = outbound_rx.recv().await {
            let payload = match serde_json::to_string(&event) {
                Ok(payload) => payload,
                Err(error) => {
                    warn!(error = %error, "failed to serialize websocket event");
                    continue;
                }
            };

            if sender.send(Message::Text(payload.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(message) = receiver.next().await {
        match message {
            Ok(Message::Text(payload)) => {
                if let Err(error) =
                    handle_client_message(payload.as_str(), &state, &topics, &outbound_tx).await
                {
                    warn!(error = %error, "failed to handle websocket client message");
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Binary(_)) => {}
            Err(error) => {
                warn!(error = %error, "websocket receive loop failed");
                break;
            }
        }
    }

    drop(outbound_tx);
    fanout_task.abort();
    let _ = fanout_task.await;
    let _ = send_task.await;
    debug!("daemon websocket connection closed");
}

async fn handle_client_message(
    payload: &str,
    state: &DaemonApiState,
    topics: &ClientTopics,
    outbound_tx: &mpsc::Sender<DaemonWsEvent>,
) -> Result<()> {
    let request: DaemonWsSubscribeRequest =
        serde_json::from_str(payload).context("invalid websocket request payload")?;

    if request.action != "subscribe" {
        return Ok(());
    }

    let normalized_topics = normalize_topics(request.topics);
    {
        let mut guard = topics.write().await;
        guard.extend(normalized_topics.iter().cloned());
    }

    for topic in normalized_topics {
        let snapshot = build_snapshot_event(state, &topic).await?;
        if outbound_tx.send(snapshot).await.is_err() {
            break;
        }
    }

    Ok(())
}

fn normalize_topics(topics: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for topic in topics {
        if !is_supported_topic(topic.as_str()) {
            continue;
        }
        if seen.insert(topic.clone()) {
            normalized.push(topic);
        }
    }

    normalized
}

fn is_supported_topic(topic: &str) -> bool {
    matches!(
        topic,
        TOPIC_STATUS | TOPIC_PEERS | TOPIC_PAIRED_DEVICES | TOPIC_PAIRING
    )
}

async fn build_snapshot_event(state: &DaemonApiState, topic: &str) -> Result<DaemonWsEvent> {
    match topic {
        TOPIC_STATUS => snapshot_event(
            TOPIC_STATUS,
            STATUS_SNAPSHOT_EVENT,
            None,
            state.query_service.status().await?,
        ),
        TOPIC_PEERS => snapshot_event(
            TOPIC_PEERS,
            PEERS_SNAPSHOT_EVENT,
            None,
            state.query_service.peers().await?,
        ),
        TOPIC_PAIRED_DEVICES => snapshot_event(
            TOPIC_PAIRED_DEVICES,
            PAIRED_DEVICES_SNAPSHOT_EVENT,
            None,
            state.query_service.paired_devices().await?,
        ),
        TOPIC_PAIRING => snapshot_event(
            TOPIC_PAIRING,
            PAIRING_SNAPSHOT_EVENT,
            None,
            state.query_service.pairing_sessions().await,
        ),
        unsupported => anyhow::bail!("unsupported websocket topic: {unsupported}"),
    }
}

fn snapshot_event<T: Serialize>(
    topic: &str,
    event_type: &str,
    session_id: Option<String>,
    payload: T,
) -> Result<DaemonWsEvent> {
    Ok(DaemonWsEvent {
        topic: topic.to_string(),
        event_type: event_type.to_string(),
        session_id,
        ts: chrono::Utc::now().timestamp_millis(),
        payload: serde_json::to_value(payload).context("failed to encode websocket payload")?,
    })
}

fn unauthorized() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "unauthorized"})),
    )
}

#[allow(dead_code)]
fn _event_type_markers(
    _: StatusResponse,
    _: Vec<PeerSnapshotDto>,
    _: Vec<PairedDeviceDto>,
    _: Vec<PairingSessionSummaryDto>,
) -> [&'static str; 4] {
    [
        STATUS_UPDATED_EVENT,
        PEERS_CHANGED_EVENT,
        PAIRED_DEVICES_CHANGED_EVENT,
        PAIRING_UPDATED_EVENT,
    ]
}
