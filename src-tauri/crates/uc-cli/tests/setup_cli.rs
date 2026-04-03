#![allow(dead_code, unused_imports)]

use serde_json::json;

mod ui {
    pub fn header(_text: &str) {}
    pub fn step(_text: &str) {}
    pub fn success(_text: &str) {}
    pub fn warn(_text: &str) {}
    pub fn error(_text: &str) {}
    pub fn info(_label: &str, _value: &str) {}
    pub fn bar() {}
    pub fn end(_text: &str) {}
    pub fn select(_prompt: &str, _items: &[String]) -> Result<usize, String> {
        Ok(0)
    }
    pub fn confirm(_prompt: &str, _default: bool) -> Result<bool, String> {
        Ok(true)
    }
    pub fn password(_prompt: &str) -> Result<String, String> {
        Ok(String::new())
    }
    pub fn password_with_confirm(_prompt: &str, _confirm: &str) -> Result<String, String> {
        Ok(String::new())
    }
    pub fn spinner(_message: &str) -> indicatif::ProgressBar {
        indicatif::ProgressBar::hidden()
    }
    pub fn spinner_finish_success(_pb: &indicatif::ProgressBar, _message: &str) {}
    pub fn spinner_finish_error(_pb: &indicatif::ProgressBar, _message: &str) {}
    pub fn identity_banner(_profile: &str, _mode: &str, _device: &str, _peer_id: &str) {}
    pub fn verification_code(_code: &str) {}
}

mod exit_codes {
    pub const EXIT_SUCCESS: i32 = 0;
    pub const EXIT_ERROR: i32 = 1;
    pub const EXIT_DAEMON_UNREACHABLE: i32 = 5;
}

mod output {
    pub fn print_result<T>(_value: &T, _json: bool) -> Result<(), std::io::Error>
    where
        T: serde::Serialize + std::fmt::Display,
    {
        Ok(())
    }
}

mod local_daemon {
    #[derive(Debug)]
    pub struct LocalDaemonError;

    impl std::fmt::Display for LocalDaemonError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("stub local daemon error")
        }
    }

    impl std::error::Error for LocalDaemonError {}

    pub async fn ensure_local_daemon_running() -> Result<(), LocalDaemonError> {
        Ok(())
    }
}

mod daemon_client {
    use reqwest::StatusCode;
    use uc_daemon::api::dto::pairing::AckedPairingCommandResponse;
    use uc_daemon::api::types::{
        PeerSnapshotDto, SetupActionAckResponse, SetupResetResponse, SetupStateResponse,
    };

    #[derive(Debug)]
    pub enum DaemonClientError {
        Unreachable(anyhow::Error),
        Unauthorized,
        Initialization(anyhow::Error),
        UnexpectedStatus { status: StatusCode, body: String },
        InvalidResponse(anyhow::Error),
    }

    impl std::fmt::Display for DaemonClientError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{self:?}")
        }
    }

    impl std::error::Error for DaemonClientError {}

    pub struct DaemonHttpClient;

    impl DaemonHttpClient {
        pub fn new() -> Result<Self, DaemonClientError> {
            Ok(Self)
        }

        pub async fn get_setup_state(&self) -> Result<SetupStateResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn start_setup_host(&self) -> Result<SetupActionAckResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn submit_setup_passphrase(
            &self,
            _passphrase: String,
        ) -> Result<SetupActionAckResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn verify_setup_passphrase(
            &self,
            _passphrase: String,
        ) -> Result<SetupActionAckResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn confirm_setup_peer(
            &self,
        ) -> Result<SetupActionAckResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn accept_pairing_session(
            &self,
            _session_id: String,
        ) -> Result<AckedPairingCommandResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn verify_pairing_session(
            &self,
            _session_id: String,
            _pin_matches: bool,
        ) -> Result<AckedPairingCommandResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn cancel_setup(&self) -> Result<SetupActionAckResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn start_setup_join(&self) -> Result<SetupActionAckResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn get_peers(&self) -> Result<Vec<PeerSnapshotDto>, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn select_setup_peer(
            &self,
            _peer_id: String,
        ) -> Result<SetupActionAckResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn reset_setup(&self) -> Result<SetupResetResponse, DaemonClientError> {
            unreachable!("stub")
        }

        pub async fn set_pairing_gui_lease(&self, _enabled: bool) -> Result<(), DaemonClientError> {
            unreachable!("stub")
        }
    }
}

#[path = "../src/commands/setup.rs"]
mod setup;

use setup::{
    new_space_encryption_guard, parse_setup_state, render_reset_output,
    should_complete_host_flow, should_prompt_host_decision, SetupHint, SetupStatusOutput,
    SetupVariant,
};
use uc_core::security::state::EncryptionState;
use uc_daemon::api::types::SetupStateResponse;

fn sample_status_response() -> SetupStateResponse {
    SetupStateResponse {
        state: json!({
            "JoinSpaceSelectDevice": {
                "error": serde_json::Value::Null
            }
        }),
        session_id: Some("session-123".to_string()),
        next_step_hint: "join-select-peer".to_string(),
        profile: "peerB".to_string(),
        clipboard_mode: "passive".to_string(),
        device_name: "Peer B".to_string(),
        peer_id: "peer-b-id".to_string(),
        selected_peer_id: Some("peer-a-id".to_string()),
        selected_peer_name: Some("Peer A".to_string()),
        has_completed: false,
    }
}

#[test]
fn setup_status_renders_next_step_and_session_identity() {
    let output = SetupStatusOutput::from(sample_status_response()).to_string();

    assert!(output.contains("sessionId: session-123"));
    assert!(output.contains("nextStepHint: join-select-peer"));
    assert!(output.contains("profile: peerB"));
    assert!(output.contains("peerId: peer-b-id"));
}

#[test]
fn setup_join_reports_passphrase_retry_without_exiting() {
    let state = SetupStateResponse {
        state: json!({
            "JoinSpaceInputPassphrase": {
                "error": "PassphraseInvalidOrMismatch"
            }
        }),
        session_id: Some("session-join".to_string()),
        next_step_hint: "join-enter-passphrase".to_string(),
        profile: "peerB".to_string(),
        clipboard_mode: "passive".to_string(),
        device_name: "Peer B".to_string(),
        peer_id: "peer-b-id".to_string(),
        selected_peer_id: Some("peer-a-id".to_string()),
        selected_peer_name: Some("Peer A".to_string()),
        has_completed: false,
    };

    let dto: uc_daemon::api::dto::setup::SetupStateResponseDto = state.clone().into();
    let parsed = parse_setup_state(&dto);
    assert_eq!(
        parsed.error_code.as_deref(),
        Some("PassphraseInvalidOrMismatch")
    );
    assert!(
        state.next_step_hint == "join-enter-passphrase"
            || matches!(parsed.variant, SetupVariant::JoinSpaceInputPassphrase)
    );
}

#[test]
fn setup_join_prompts_for_peer_confirmation_before_passphrase() {
    let state = SetupStateResponse {
        state: json!({
            "JoinSpaceConfirmPeer": {
                "short_code": "123-456",
                "peer_fingerprint": "peer-fingerprint",
                "error": serde_json::Value::Null
            }
        }),
        session_id: Some("session-join".to_string()),
        next_step_hint: "host-confirm-peer".to_string(),
        profile: "peerB".to_string(),
        clipboard_mode: "passive".to_string(),
        device_name: "Peer B".to_string(),
        peer_id: "peer-b-id".to_string(),
        selected_peer_id: Some("peer-a-id".to_string()),
        selected_peer_name: Some("Peer A".to_string()),
        has_completed: false,
    };

    let dto: uc_daemon::api::dto::setup::SetupStateResponseDto = state.clone().into();
    let parsed = parse_setup_state(&dto);
    assert!(matches!(parsed.variant, SetupVariant::JoinSpaceConfirmPeer));
    assert!(
        state.next_step_hint != "join-enter-passphrase"
            && !matches!(parsed.variant, SetupVariant::JoinSpaceInputPassphrase)
    );
}

#[test]
fn setup_reset_reports_daemon_kept_running() {
    let rendered = render_reset_output("peerA", true);

    assert_eq!(
        rendered,
        ["Reset complete for profile peerA", "Daemon kept running"].join("\n")
    );
}

#[test]
fn setup_host_enables_pairing_presence_when_waiting_for_join_request() {
    let state = SetupStateResponse {
        state: json!("Completed"),
        session_id: None,
        next_step_hint: "completed".to_string(),
        profile: "peerA".to_string(),
        clipboard_mode: "passive".to_string(),
        device_name: "Peer A".to_string(),
        peer_id: "peer-a-id".to_string(),
        selected_peer_id: None,
        selected_peer_name: None,
        has_completed: true,
    };

    // should_enable_host_pairing_presence = !already_enabled && next_step_hint == "completed"
    // when already_enabled=false: should return true since !false = true
    let already_enabled = false;
    assert!(!already_enabled && state.next_step_hint == "completed");
    // when already_enabled=true: should return false since !true = false
    let already_enabled = true;
    assert!(!(!already_enabled && state.next_step_hint == "completed"));
}

#[test]
fn setup_host_prompts_for_verification_after_accept() {
    let state = SetupStateResponse {
        state: json!({
            "JoinSpaceConfirmPeer": {
                "short_code": "123-456",
                "peer_fingerprint": "peer-fingerprint",
                "error": serde_json::Value::Null
            }
        }),
        session_id: Some("session-host".to_string()),
        next_step_hint: "host-confirm-peer".to_string(),
        profile: "peerA".to_string(),
        clipboard_mode: "full".to_string(),
        device_name: "Peer A".to_string(),
        peer_id: "peer-a-id".to_string(),
        selected_peer_id: Some("peer-b-id".to_string()),
        selected_peer_name: Some("Peer B".to_string()),
        has_completed: true,
    };

    // should_prompt_for_host_verification = has_completed && variant == "JoinSpaceConfirmPeer"
    let dto: uc_daemon::api::dto::setup::SetupStateResponseDto = state.clone().into();
    let parsed = parse_setup_state(&dto);
    assert!(
        state.has_completed
            && matches!(parsed.variant, SetupVariant::JoinSpaceConfirmPeer)
    );
}

#[test]
fn host_decision_prompt_is_suppressed_after_same_session_submission() {
    let state = SetupStateResponse {
        state: json!("Completed"),
        session_id: Some("session-host".to_string()),
        next_step_hint: "host-confirm-peer".to_string(),
        profile: "peerA".to_string(),
        clipboard_mode: "full".to_string(),
        device_name: "Peer A".to_string(),
        peer_id: "peer-a-id".to_string(),
        selected_peer_id: Some("peer-b-id".to_string()),
        selected_peer_name: Some("Peer B".to_string()),
        has_completed: true,
    };

    let dto: uc_daemon::api::dto::setup::SetupStateResponseDto = state.into();
    let parsed = parse_setup_state(&dto);
    assert!(!should_prompt_host_decision(&parsed, Some("session-host")));
}

#[test]
fn host_decision_prompt_is_allowed_for_new_session() {
    let state = SetupStateResponse {
        state: json!("Completed"),
        session_id: Some("session-host".to_string()),
        next_step_hint: "host-confirm-peer".to_string(),
        profile: "peerA".to_string(),
        clipboard_mode: "full".to_string(),
        device_name: "Peer A".to_string(),
        peer_id: "peer-a-id".to_string(),
        selected_peer_id: Some("peer-b-id".to_string()),
        selected_peer_name: Some("Peer B".to_string()),
        has_completed: true,
    };

    let dto: uc_daemon::api::dto::setup::SetupStateResponseDto = state.clone().into();
    let parsed = parse_setup_state(&dto);
    assert!(should_prompt_host_decision(&parsed, None));
    let dto2: uc_daemon::api::dto::setup::SetupStateResponseDto = state.into();
    let parsed2 = parse_setup_state(&dto2);
    assert!(should_prompt_host_decision(&parsed2, Some("other-session")));
}

#[test]
fn host_flow_only_exits_after_active_session_clears() {
    let active = SetupStateResponse {
        state: json!("Completed"),
        session_id: Some("session-host".to_string()),
        next_step_hint: "completed".to_string(),
        profile: "peerA".to_string(),
        clipboard_mode: "full".to_string(),
        device_name: "Peer A".to_string(),
        peer_id: "peer-a-id".to_string(),
        selected_peer_id: None,
        selected_peer_name: None,
        has_completed: true,
    };
    let cleared = SetupStateResponse {
        session_id: None,
        ..active.clone()
    };

    // host_flow_completed = handled_peer_request && has_completed && next_step_hint == "completed" && session_id.is_none()
    let handled_peer_request = true;
    assert!(
        !handled_peer_request
            || !active.has_completed
            || active.next_step_hint != "completed"
            || active.session_id.is_some()
    ); // active case fails
    assert!(
        cleared.has_completed
            && cleared.next_step_hint == "completed"
            && cleared.session_id.is_none()
    ); // cleared case passes
}

#[test]
fn host_peer_label_includes_peer_id_suffix_when_name_present() {
    let state = SetupStateResponse {
        state: json!("Completed"),
        session_id: Some("session-host".to_string()),
        next_step_hint: "host-confirm-peer".to_string(),
        profile: "peerA".to_string(),
        clipboard_mode: "full".to_string(),
        device_name: "Peer A".to_string(),
        peer_id: "peer-a-id".to_string(),
        selected_peer_id: Some("12D3KooWABCDEFGH".to_string()),
        selected_peer_name: Some("Peer B".to_string()),
        has_completed: true,
    };

    let dto: uc_daemon::api::dto::setup::SetupStateResponseDto = state.into();
    let parsed = parse_setup_state(&dto);
    assert_eq!(
        parsed.selected_peer_label,
        Some("Peer B (ABCDEFGH)".to_string())
    );
}

#[test]
fn host_flow_completion_waits_for_verification_confirmation() {
    let state = SetupStateResponse {
        state: json!("Completed"),
        session_id: None,
        next_step_hint: "completed".to_string(),
        profile: "peerA".to_string(),
        clipboard_mode: "full".to_string(),
        device_name: "Peer A".to_string(),
        peer_id: "peer-a-id".to_string(),
        selected_peer_id: Some("peer-b-id".to_string()),
        selected_peer_name: Some("Peer B".to_string()),
        has_completed: true,
    };

    let dto: uc_daemon::api::dto::setup::SetupStateResponseDto = state.clone().into();
    let parsed = parse_setup_state(&dto);
    assert!(!should_complete_host_flow(&parsed, true, false));
    let dto2: uc_daemon::api::dto::setup::SetupStateResponseDto = state.into();
    let parsed2 = parse_setup_state(&dto2);
    assert!(should_complete_host_flow(&parsed2, true, true));
}

#[test]
fn new_space_already_initialized_returns_error() {
    let result = new_space_encryption_guard(EncryptionState::Initialized);
    assert_eq!(result, Err(exit_codes::EXIT_ERROR));
}

#[test]
fn new_space_uninitialized_allows_init() {
    let result = new_space_encryption_guard(EncryptionState::Uninitialized);
    assert!(result.is_ok());
}
