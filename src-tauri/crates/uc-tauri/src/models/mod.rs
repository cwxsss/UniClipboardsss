//! Data Transfer Objects and Projection Models
//!
//! This module contains data structures that are exposed to the frontend.
//! These separate the internal domain models from the API contract.
//!
//! 数据传输对象和投影模型
//!
//! 此模块包含暴露给前端的数据结构。
//! 这些将内部领域模型与 API 契约分离。

use serde::{Deserialize, Serialize};
use uc_app::usecases::LifecycleState;

/// Lifecycle status DTO for the frontend API.
/// 前端 API 的生命周期状态 DTO。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleStatusDto {
    /// Current lifecycle state (e.g. "Idle", "Ready", "NetworkFailed", etc.)
    pub state: LifecycleState,
}

impl LifecycleStatusDto {
    pub fn from_state(state: LifecycleState) -> Self {
        Self { state }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_status_dto_serializes_with_camel_case() {
        // The struct field "state" is already one word, but we verify camelCase rename_all is applied
        let dto = LifecycleStatusDto {
            state: LifecycleState::Ready,
        };
        let value = serde_json::to_value(&dto).expect("serialize failed");
        // Verify it has "state" key (camelCase of "state" is still "state")
        assert!(
            value.get("state").is_some(),
            "expected 'state' field in JSON"
        );
        assert_eq!(value["state"], serde_json::json!("Ready"));

        // Verify all variants serialize as expected
        let idle = LifecycleStatusDto::from_state(LifecycleState::Idle);
        let idle_json = serde_json::to_value(&idle).expect("serialize failed");
        assert_eq!(idle_json["state"], serde_json::json!("Idle"));
    }
}
