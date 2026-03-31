//! HTTP route handlers for lifecycle management endpoints.
//!
//! Provides GET /lifecycle/status, POST /lifecycle/retry, and POST /lifecycle/ready.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use std::sync::atomic::Ordering;
use tracing::{info, warn, Instrument};
use uc_app::usecases::CoreUseCases;

use super::types::LifecycleStatusResponse;
use crate::api::routes::internal_error;
use crate::api::server::DaemonApiState;

/// Build the lifecycle router for daemon HTTP API.
pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route("/lifecycle/status", get(get_lifecycle_status_handler))
        .route("/lifecycle/retry", post(retry_lifecycle_handler))
        .route("/lifecycle/ready", post(lifecycle_ready_handler))
}

/// Signal that the GUI has unlocked and clipboard capture can begin.
///
/// In `--gui-managed` mode, clipboard capture is gated until the GUI
/// explicitly signals readiness (after the user unlocks the app).
/// This endpoint opens that gate.
async fn lifecycle_ready_handler(State(state): State<DaemonApiState>) -> impl IntoResponse {
    if let Some(gate) = &state.clipboard_capture_gate {
        let was_closed = !gate.swap(true, Ordering::SeqCst);
        if was_closed {
            info!("Clipboard capture gate opened by GUI lifecycle/ready signal");
        } else {
            info!("Clipboard capture gate already open (duplicate lifecycle/ready call)");
        }
    }

    // Trigger deferred services start (clipboard-watcher, inbound-clipboard-sync, etc.)
    if let Some(notify) = &state.deferred_ready_notify {
        notify.notify_one();
    }

    StatusCode::NO_CONTENT.into_response()
}

/// GET /lifecycle/status
/// Returns the current lifecycle state as a plain JSON object.
async fn get_lifecycle_status_handler(State(state): State<DaemonApiState>) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return internal_error(anyhow::anyhow!("daemon runtime unavailable")).into_response();
    };

    let usecases = CoreUseCases::new(runtime.as_ref());
    let status_port = usecases.get_lifecycle_status();
    let current_state = status_port.get_state().await;

    Json(LifecycleStatusResponse {
        state: format!("{current_state:?}"),
    })
    .into_response()
}

/// POST /lifecycle/retry
/// Retries the lifecycle boot by calling ensure_ready() on the AppLifecycleCoordinator.
/// Re-constructs the coordinator from daemon-owned components (no TauriSessionReadyEmitter needed).
async fn retry_lifecycle_handler(State(state): State<DaemonApiState>) -> impl IntoResponse {
    let Some(runtime) = state.runtime.clone() else {
        return internal_error(anyhow::anyhow!("daemon runtime unavailable")).into_response();
    };

    let span = tracing::info_span!("daemon.lifecycle.retry");
    async {
        let usecases = CoreUseCases::new(runtime.as_ref());

        // Check if already ready — skip if so.
        let status_port = usecases.get_lifecycle_status();
        if status_port.get_state().await == uc_app::usecases::app_lifecycle::LifecycleState::Ready {
            info!("Lifecycle already Ready; skipping retry");
            return StatusCode::NO_CONTENT.into_response();
        }

        // Mark as Pending.
        if let Err(e) = status_port
            .set_state(uc_app::usecases::app_lifecycle::LifecycleState::Pending)
            .await
        {
            warn!(error = %e, "Failed to set lifecycle state to Pending");
        }

        // Start the network runtime.
        let network = usecases.start_network_after_unlock();
        if let Err(e) = network.execute().await {
            let msg = e.to_string();
            if !msg.to_ascii_lowercase().contains("already started") {
                warn!(error = %msg, "Network failed to start during lifecycle retry");
                if let Err(e) = status_port
                    .set_state(uc_app::usecases::app_lifecycle::LifecycleState::NetworkFailed)
                    .await
                {
                    warn!(error = %e, "Failed to set lifecycle state to NetworkFailed");
                }
                return internal_error(anyhow::anyhow!(msg)).into_response();
            }
            info!(error = %msg, "Network already started; continuing");
        }

        // All good – mark ready.
        if let Err(e) = status_port
            .set_state(uc_app::usecases::app_lifecycle::LifecycleState::Ready)
            .await
        {
            warn!(error = %e, "Failed to set lifecycle state to Ready");
        }

        // Signal clipboard capture gate (if present) — same as /lifecycle/ready.
        if let Some(gate) = &state.clipboard_capture_gate {
            let was_closed = !gate.swap(true, std::sync::atomic::Ordering::SeqCst);
            if was_closed {
                info!("Clipboard capture gate opened by lifecycle retry");
            }
        }

        // Trigger deferred services start.
        if let Some(notify) = &state.deferred_ready_notify {
            notify.notify_one();
        }

        info!("Lifecycle retry completed successfully");
        StatusCode::NO_CONTENT.into_response()
    }
    .instrument(span)
    .await
}
