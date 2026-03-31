//! Event Forwarding - Forward backend events to frontend
//! 事件转发 - 将后端事件转发到前端

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Encryption events emitted to frontend
/// 发送到前端的加密事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EncryptionEvent {
    /// Encryption session ready (keyring unlock completed)
    SessionReady,
}

/// Forward encryption event to frontend
/// 将加密事件转发到前端
pub fn forward_encryption_event<R: tauri::Runtime>(
    app: &AppHandle<R>,
    event: EncryptionEvent,
) -> Result<(), Box<dyn std::error::Error>> {
    app.emit("encryption://event", event)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_event_serializes_with_type_tag() {
        let ready = serde_json::to_value(EncryptionEvent::SessionReady).unwrap();
        assert_eq!(ready, serde_json::json!({ "type": "SessionReady" }));
    }
}
