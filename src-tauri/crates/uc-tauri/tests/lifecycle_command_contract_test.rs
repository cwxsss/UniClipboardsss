//! Contract tests for lifecycle commands.
//! 生命周期命令契约测试：验证 get_lifecycle_status / CommandError JSON 结构。

use serde_json::Value;
use uc_app::usecases::LifecycleState;
use uc_tauri::commands::CommandError;
use uc_tauri::models::LifecycleStatusDto;

#[test]
fn lifecycle_status_dto_json_shape_matches_contract() {
    let dto = LifecycleStatusDto::from_state(LifecycleState::Ready);
    let json = serde_json::to_value(&dto).expect("serialize failed");

    // JSON 顶层必须是对象，包含 state 字段，值为枚举字符串
    assert!(
        json.is_object(),
        "LifecycleStatusDto must serialize to JSON object"
    );

    assert!(
        json.get("state").is_some(),
        "expected 'state' field in LifecycleStatusDto JSON"
    );
    assert_eq!(json["state"], Value::String("Ready".to_string()));

    // Spot-check other variants to guard enum value stability
    let idle = LifecycleStatusDto::from_state(LifecycleState::Idle);
    let idle_json = serde_json::to_value(&idle).expect("serialize failed");
    assert_eq!(idle_json["state"], Value::String("Idle".to_string()));

    let watcher_failed = LifecycleStatusDto::from_state(LifecycleState::WatcherFailed);
    let wf_json = serde_json::to_value(&watcher_failed).expect("serialize failed");
    assert_eq!(wf_json["state"], Value::String("WatcherFailed".to_string()));

    let network_failed = LifecycleStatusDto::from_state(LifecycleState::NetworkFailed);
    let nf_json = serde_json::to_value(&network_failed).expect("serialize failed");
    assert_eq!(nf_json["state"], Value::String("NetworkFailed".to_string()));
}

#[test]
fn command_error_serialization_matches_discriminated_union_contract() {
    let not_found = CommandError::NotFound("missing".to_string());
    let nf_json = serde_json::to_value(&not_found).expect("serialize CommandError::NotFound");

    assert_eq!(nf_json["code"], Value::String("NotFound".to_string()));
    assert!(
        nf_json.get("message").is_some(),
        "CommandError must include 'message' field"
    );

    let internal = CommandError::InternalError("boom".to_string());
    let internal_json =
        serde_json::to_value(&internal).expect("serialize CommandError::InternalError");
    assert_eq!(
        internal_json["code"],
        Value::String("InternalError".to_string())
    );

    // Ensure JSON shape is flat { code, message } and does not nest details
    assert!(nf_json.get("message").is_some());
    assert!(nf_json.get("detail").is_none());
}
