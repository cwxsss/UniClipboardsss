//! Tauri-specific lifecycle adapter.
//!
//! Pure (non-Tauri) lifecycle adapters (`InMemoryLifecycleStatus`,
//! `LoggingLifecycleEventEmitter`, `LoggingSessionReadyEmitter`,
//! `DeviceNameAnnouncer`) have been moved to
//! `uc_app::usecases::app_lifecycle::adapters`.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tauri::{AppHandle, Runtime};
use uc_app::usecases::SessionReadyEmitter;

pub struct TauriSessionReadyEmitter<R: Runtime> {
    app_handle: Arc<std::sync::RwLock<Option<AppHandle<R>>>>,
}

impl<R: Runtime> TauriSessionReadyEmitter<R> {
    pub fn new(app_handle: Arc<std::sync::RwLock<Option<AppHandle<R>>>>) -> Self {
        Self { app_handle }
    }
}

#[async_trait]
impl<R: Runtime> SessionReadyEmitter for TauriSessionReadyEmitter<R> {
    async fn emit_ready(&self) -> Result<()> {
        let guard = self.app_handle.read().unwrap_or_else(|poisoned| {
            tracing::error!(
                "RwLock poisoned in session ready emission, recovering from poisoned state"
            );
            poisoned.into_inner()
        });

        if let Some(app) = guard.as_ref() {
            if let Err(err) = crate::events::forward_encryption_event(
                app,
                crate::events::EncryptionEvent::SessionReady,
            ) {
                tracing::warn!(error = %err, "Failed to emit encryption session ready event");
            }
        } else {
            tracing::warn!("AppHandle not available, skipping encryption session ready emission");
        }

        tracing::info!("Session ready");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use tauri::Listener;

    #[tokio::test]
    async fn tauri_session_ready_emitter_emits_frontend_event() {
        let app = tauri::test::mock_app();
        let app_handle = app.handle();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1);

        let tx_clone = tx.clone();
        app_handle.listen("encryption://event", move |event: tauri::Event| {
            let _ = tx_clone.try_send(event.payload().to_string());
        });

        let emitter = TauriSessionReadyEmitter::new(Arc::new(std::sync::RwLock::new(Some(
            app_handle.clone(),
        ))));
        emitter.emit_ready().await.unwrap();

        let payload = rx.recv().await.expect("event payload");
        let value: Value = serde_json::from_str(&payload).expect("json payload");
        assert_eq!(value, serde_json::json!({ "type": "SessionReady" }));
    }

    #[tokio::test]
    async fn tauri_session_ready_emitter_without_handle_is_ok() {
        let app = tauri::test::mock_app();
        let app_handle = app.handle();
        let emitter = TauriSessionReadyEmitter::new(Arc::new(std::sync::RwLock::new(Some(
            app_handle.clone(),
        ))));

        *emitter
            .app_handle
            .write()
            .expect("rwlock should not be poisoned") = None;

        let result = emitter.emit_ready().await;
        assert!(result.is_ok());
    }
}
