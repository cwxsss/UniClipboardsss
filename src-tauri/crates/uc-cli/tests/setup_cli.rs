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
    use uc_daemon::api::pairing::AckedPairingCommandResponse;
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
    host_flow_completed, join_retry_message, new_space_encryption_guard, render_reset_output,
    should_enable_host_pairing_presence, should_prompt_for_host_verification,
    should_prompt_for_join_passphrase, should_prompt_for_join_peer_confirmation, SetupStatusOutput,
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

    assert_eq!(
        join_retry_message(&state),
        Some("Passphrase rejected; retrying current join session")
    );
    assert!(should_prompt_for_join_passphrase(&state));
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

    assert!(should_prompt_for_join_peer_confirmation(&state));
    assert!(!should_prompt_for_join_passphrase(&state));
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

    assert!(should_enable_host_pairing_presence(&state, false));
    assert!(!should_enable_host_pairing_presence(&state, true));
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

    assert!(should_prompt_for_host_verification(&state));
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

    assert!(!host_flow_completed(&active, true));
    assert!(host_flow_completed(&cleared, true));
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
