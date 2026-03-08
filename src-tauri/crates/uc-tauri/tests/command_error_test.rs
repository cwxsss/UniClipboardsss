use uc_tauri::commands::CommandError;

#[test]
fn not_found_serializes_with_code_and_message() {
    let err = CommandError::NotFound("entry-1".to_string());
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "NotFound");
    assert!(json["message"]
        .as_str()
        .map(|s| s.contains("entry-1"))
        .unwrap_or(false));
}

#[test]
fn internal_error_has_internal_error_code() {
    let err = CommandError::InternalError("boom".to_string());
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "InternalError");
}

#[test]
fn cancelled_distinct_from_internal_error() {
    let cancelled = serde_json::to_value(&CommandError::Cancelled("c".to_string())).unwrap();
    let internal = serde_json::to_value(&CommandError::InternalError("i".to_string())).unwrap();
    assert_ne!(cancelled["code"], internal["code"]);
    assert_eq!(cancelled["code"], "Cancelled");
}

#[test]
fn timeout_has_timeout_code() {
    let err = CommandError::Timeout("deadline exceeded".to_string());
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["code"], "Timeout");
}

#[test]
fn display_format_includes_message() {
    let err = CommandError::NotFound("missing-id".to_string());
    let display = err.to_string();
    assert!(display.contains("missing-id"), "display: {display}");
}
