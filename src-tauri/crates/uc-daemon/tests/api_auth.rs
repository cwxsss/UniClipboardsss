use std::fs;

use uc_daemon::api::auth::{
    build_connection_info, load_or_create_auth_token, parse_bearer_token, resolve_daemon_token_path,
};

#[test]
fn load_creates_token_when_missing() {
    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");

    let token = load_or_create_auth_token(&token_path).unwrap();

    assert!(!token.as_str().is_empty());
    assert_eq!(fs::read_to_string(&token_path).unwrap(), token.as_str());
}

#[test]
fn load_reuses_existing_token() {
    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");
    fs::write(&token_path, "persisted-token").unwrap();

    let token = load_or_create_auth_token(&token_path).unwrap();

    assert_eq!(token.as_str(), "persisted-token");
}

#[test]
fn bearer_header_parser_accepts_bearer_and_rejects_malformed_values() {
    assert_eq!(parse_bearer_token("Bearer abc"), Some("abc"));
    assert_eq!(parse_bearer_token("bearer abc"), None);
    assert_eq!(parse_bearer_token("Bearer"), None);
    assert_eq!(parse_bearer_token("Basic abc"), None);
}

#[test]
fn connection_info_builder_returns_http_and_ws_urls() {
    let tempdir = tempfile::tempdir().unwrap();
    let token_path = tempdir.path().join("daemon.token");
    let token = load_or_create_auth_token(&token_path).unwrap();

    let info = build_connection_info("127.0.0.1", 43210, &token);

    assert_eq!(info.base_url, "http://127.0.0.1:43210");
    assert_eq!(info.ws_url, "ws://127.0.0.1:43210/ws");
    assert_eq!(info.token, token.as_str());
}

#[test]
fn token_path_helper_ends_with_uniclipboard_daemon_token() {
    let path = resolve_daemon_token_path(std::path::Path::new("/tmp/uniclipboard"));

    assert_eq!(
        path.file_name().and_then(std::ffi::OsStr::to_str),
        Some("uniclipboard-daemon.token")
    );
}
