//! Interactive setup commands over daemon-owned setup state.

use std::fmt;
use std::io::{self, IsTerminal};
use std::time::Duration;

use console::style;
use serde::Serialize;
use serde_json::Value;
use uc_core::security::state::EncryptionState;
use uc_daemon::api::types::{PeerSnapshotDto, SetupStateResponse};

use crate::daemon_client::{DaemonClientError, DaemonHttpClient};
use crate::exit_codes;
use crate::local_daemon::{ensure_local_daemon_running, LocalDaemonError};
use crate::output;
use crate::ui;

const POLL_INTERVAL: Duration = Duration::from_millis(400);
const HOST_LEASE_REFRESH_INTERVAL: Duration = Duration::from_secs(20);

// ── Interactive guide (no subcommand) ───────────────────────────────

pub async fn run_interactive(json: bool, verbose: bool) -> i32 {
    if json {
        eprintln!("Error: `--json` is only supported with `setup status`");
        return exit_codes::EXIT_ERROR;
    }
    if !stdin_is_terminal() {
        eprintln!("Error: interactive setup requires a terminal");
        return exit_codes::EXIT_ERROR;
    }

    ui::header("Welcome to UniClipboard");

    let items = vec![
        "Create new Space (I'm the first device)".to_string(),
        "Join existing Space (connect to another device)".to_string(),
    ];

    let choice = match ui::select("What would you like to do?", &items) {
        Ok(choice) => choice,
        Err(e) => {
            ui::error(&format!("Setup cancelled: {e}"));
            return exit_codes::EXIT_ERROR;
        }
    };

    ui::bar();

    match choice {
        0 => run_new_space().await,
        1 => run_join(json, verbose).await,
        _ => unreachable!(),
    }
}

// ── New Space flow (create encrypted space only, no pairing) ────────

/// Returns `Ok(())` if encryption state allows new-space initialization,
/// or `Err(exit_code)` if the operation should be rejected.
pub fn new_space_encryption_guard(state: EncryptionState) -> Result<(), i32> {
    if state == EncryptionState::Initialized {
        Err(exit_codes::EXIT_ERROR)
    } else {
        Ok(())
    }
}

async fn run_new_space() -> i32 {
    let spinner = ui::spinner("Starting daemon…");
    if let Err(error) = ensure_local_daemon_running().await {
        ui::spinner_finish_error(&spinner, &format!("{error}"));
        return exit_codes::EXIT_DAEMON_UNREACHABLE;
    }

    let client = match DaemonHttpClient::new() {
        Ok(client) => client,
        Err(error) => {
            ui::spinner_finish_error(&spinner, &format!("{error}"));
            return print_client_error(error);
        }
    };
    ui::spinner_finish_success(&spinner, "Daemon ready");

    // Transition state machine to CreateSpaceInputPassphrase
    let ack = match client.start_setup_host().await {
        Ok(ack) => ack,
        Err(error) => return print_client_error(error),
    };

    if ack.next_step_hint != "create-space-passphrase"
        && !matches!(
            setup_state_variant(&ack.state),
            Some("CreateSpaceInputPassphrase" | "ProcessingCreateSpace")
        )
    {
        ui::error("Unexpected setup state — space may already be created");
        ui::info("Hint", "run `setup reset` first if you want to start over");
        return exit_codes::EXIT_ERROR;
    }

    // Prompt for passphrase
    let passphrase = match prompt_new_space_passphrase() {
        Ok(p) => p,
        Err(error) => {
            ui::error(&error);
            return exit_codes::EXIT_ERROR;
        }
    };

    // Submit passphrase — daemon creates encrypted space
    let spinner = ui::spinner("Creating encrypted space…");
    if let Err(error) = client.submit_setup_passphrase(passphrase).await {
        ui::spinner_finish_error(&spinner, "Failed to create space");
        return print_client_error(error);
    }

    // Poll until state reaches Completed
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let state = match client.get_setup_state().await {
            Ok(s) => s,
            Err(error) => {
                ui::spinner_finish_error(&spinner, "Failed to check setup state");
                return print_client_error(error);
            }
        };

        if state.has_completed || state.next_step_hint == "completed" {
            ui::spinner_finish_success(&spinner, "Encrypted space created");
            ui::bar();
            ui::success("Setup complete! Your space is ready.");
            ui::info(
                "Next step",
                "run `setup host` on this device, then `setup join` on another device to pair",
            );
            ui::end("");
            return exit_codes::EXIT_SUCCESS;
        }

        if std::time::Instant::now() >= deadline {
            ui::spinner_finish_error(&spinner, "Timed out waiting for space creation");
            return exit_codes::EXIT_ERROR;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ── Host flow ───────────────────────────────────────────────────────

pub async fn run_host(json: bool, _verbose: bool) -> i32 {
    if json {
        eprintln!("Error: `--json` is only supported with `setup status`");
        return exit_codes::EXIT_ERROR;
    }
    if !stdin_is_terminal() {
        eprintln!("Error: `setup host` requires an interactive terminal");
        return exit_codes::EXIT_ERROR;
    }

    if let Err(error) = ensure_local_daemon_running().await {
        return print_local_daemon_error(error);
    }

    let client = match DaemonHttpClient::new() {
        Ok(client) => client,
        Err(error) => return print_client_error(error),
    };

    let initial_state = match client.get_setup_state().await {
        Ok(state) => state,
        Err(error) => return print_client_error(error),
    };

    ui::step("Device identity");
    print_identity_banner(&initial_state);

    let mut ack = match client.start_setup_host().await {
        Ok(ack) => ack,
        Err(error) => return print_client_error(error),
    };

    if ack.next_step_hint == "create-space-passphrase"
        || matches!(
            setup_state_variant(&ack.state),
            Some("CreateSpaceInputPassphrase" | "ProcessingCreateSpace")
        )
    {
        let passphrase = match prompt_new_space_passphrase() {
            Ok(passphrase) => passphrase,
            Err(error) => {
                ui::error(&error);
                return exit_codes::EXIT_ERROR;
            }
        };
        ack = match client.submit_setup_passphrase(passphrase).await {
            Ok(ack) => ack,
            Err(error) => return print_client_error(error),
        };
    }

    let mut last_signature = String::new();
    let mut handled_peer_request = false;
    let mut host_pairing_presence_enabled = false;
    let mut last_host_lease_refresh = std::time::Instant::now();
    let mut spinner: Option<indicatif::ProgressBar> = None;

    loop {
        let state = match client.get_setup_state().await {
            Ok(state) => state,
            Err(error) => {
                finish_spinner(&mut spinner);
                return print_client_error(error);
            }
        };

        let signature = state_signature(&state);
        if signature != last_signature {
            last_signature = signature;
        }

        if should_prompt_for_host_verification(&state) {
            finish_spinner(&mut spinner);
            handled_peer_request = true;
            let session_id = match state.session_id.clone() {
                Some(session_id) => session_id,
                None => {
                    ui::error("Missing pairing session id for host verification");
                    return exit_codes::EXIT_ERROR;
                }
            };
            match prompt_host_verification(&state) {
                Ok(true) => {
                    if let Err(error) = client.verify_pairing_session(session_id, true).await {
                        return print_client_error(error);
                    }
                }
                Ok(false) => {
                    if let Err(error) = client.verify_pairing_session(session_id, false).await {
                        return print_client_error(error);
                    }
                    ui::warn("Host pairing canceled.");
                    return exit_codes::EXIT_SUCCESS;
                }
                Err(error) => {
                    ui::error(&error);
                    return exit_codes::EXIT_ERROR;
                }
            }
        } else if state.next_step_hint == "host-confirm-peer" {
            finish_spinner(&mut spinner);
            handled_peer_request = true;
            match prompt_host_decision(&state) {
                Ok(HostDecision::Accept) => {
                    let accept_result = match client.confirm_setup_peer().await {
                        Ok(_) => Ok(()),
                        Err(DaemonClientError::UnexpectedStatus { status, .. })
                            if status == reqwest::StatusCode::CONFLICT && state.has_completed =>
                        {
                            match state.session_id.clone() {
                                Some(session_id) => {
                                    client.accept_pairing_session(session_id).await.map(|_| ())
                                }
                                None => Err(DaemonClientError::UnexpectedStatus {
                                    status,
                                    body: "missing setup session id for pairing accept fallback"
                                        .to_string(),
                                }),
                            }
                        }
                        Err(error) => Err(error),
                    };
                    if let Err(error) = accept_result {
                        return print_client_error(error);
                    }
                }
                Ok(HostDecision::Reject) => {
                    if let Err(error) = client.cancel_setup().await {
                        return print_client_error(error);
                    }
                    let _ =
                        disable_host_pairing_presence(&client, &mut host_pairing_presence_enabled)
                            .await;
                    ui::warn("Host setup canceled.");
                    return exit_codes::EXIT_SUCCESS;
                }
                Err(error) => {
                    ui::error(&error);
                    return exit_codes::EXIT_ERROR;
                }
            }
        } else if state.next_step_hint == "completed" && !handled_peer_request {
            if should_enable_host_pairing_presence(&state, host_pairing_presence_enabled) {
                if let Err(error) = client.set_pairing_gui_lease(true).await {
                    finish_spinner(&mut spinner);
                    return print_client_error(error);
                }
                host_pairing_presence_enabled = true;
                last_host_lease_refresh = std::time::Instant::now();
            } else if host_pairing_presence_enabled
                && last_host_lease_refresh.elapsed() >= HOST_LEASE_REFRESH_INTERVAL
            {
                if let Err(error) = client.set_pairing_gui_lease(true).await {
                    finish_spinner(&mut spinner);
                    return print_client_error(error);
                }
                last_host_lease_refresh = std::time::Instant::now();
            }
            if spinner.is_none() {
                spinner = Some(ui::spinner("Host ready. Waiting for a join request…"));
            }
        } else if host_flow_completed(&state, handled_peer_request) {
            finish_spinner(&mut spinner);
            let _ =
                disable_host_pairing_presence(&client, &mut host_pairing_presence_enabled).await;
            ui::success("Setup host flow completed!");
            return exit_codes::EXIT_SUCCESS;
        } else if state.next_step_hint == "idle" && handled_peer_request {
            finish_spinner(&mut spinner);
            let _ =
                disable_host_pairing_presence(&client, &mut host_pairing_presence_enabled).await;
            ui::end("Host setup returned to idle.");
            return exit_codes::EXIT_SUCCESS;
        }

        let _ = &ack;
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ── Join flow ───────────────────────────────────────────────────────

pub async fn run_join(json: bool, _verbose: bool) -> i32 {
    if json {
        eprintln!("Error: `--json` is only supported with `setup status`");
        return exit_codes::EXIT_ERROR;
    }
    if !stdin_is_terminal() {
        eprintln!("Error: `setup join` requires an interactive terminal");
        return exit_codes::EXIT_ERROR;
    }

    if let Err(error) = ensure_local_daemon_running().await {
        return print_local_daemon_error(error);
    }

    let client = match DaemonHttpClient::new() {
        Ok(client) => client,
        Err(error) => return print_client_error(error),
    };

    let initial_state = match client.get_setup_state().await {
        Ok(state) => state,
        Err(error) => return print_client_error(error),
    };

    ui::step("Device identity");
    print_identity_banner(&initial_state);

    if let Err(error) = client.start_setup_join().await {
        return print_client_error(error);
    }

    let mut last_signature = String::new();
    let mut submitted_peer_request = false;
    let mut spinner: Option<indicatif::ProgressBar> = None;

    loop {
        let state = match client.get_setup_state().await {
            Ok(state) => state,
            Err(error) => {
                finish_spinner(&mut spinner);
                return print_client_error(error);
            }
        };

        let signature = state_signature(&state);
        if signature != last_signature {
            last_signature = signature;
        }

        if state.has_completed || state.next_step_hint == "completed" {
            finish_spinner(&mut spinner);
            ui::success("Setup join flow completed!");
            return exit_codes::EXIT_SUCCESS;
        }

        if state.next_step_hint == "join-select-peer" {
            let peers = match client.get_peers().await {
                Ok(peers) => filter_joinable_peers(peers),
                Err(error) => {
                    finish_spinner(&mut spinner);
                    return print_client_error(error);
                }
            };
            if peers.is_empty() {
                if spinner.is_none() {
                    spinner = Some(ui::spinner("Discovering peers on the network…"));
                }
            } else {
                finish_spinner(&mut spinner);
                match prompt_for_peer_selection(&peers) {
                    Ok(Some(peer_id)) => {
                        submitted_peer_request = true;
                        spinner = Some(ui::spinner("Connecting to peer…"));
                        if let Err(error) = client.select_setup_peer(peer_id).await {
                            finish_spinner(&mut spinner);
                            return print_client_error(error);
                        }
                    }
                    Ok(None) => {
                        if let Err(error) = client.cancel_setup().await {
                            return print_client_error(error);
                        }
                        ui::warn("Join setup canceled.");
                        return exit_codes::EXIT_SUCCESS;
                    }
                    Err(error) => {
                        ui::error(&error);
                        return exit_codes::EXIT_ERROR;
                    }
                }
            }
        } else if should_prompt_for_join_peer_confirmation(&state) {
            finish_spinner(&mut spinner);
            match prompt_join_peer_confirmation(&state) {
                Ok(true) => {
                    if let Err(error) = client.confirm_setup_peer().await {
                        return print_client_error(error);
                    }
                }
                Ok(false) => {
                    if let Err(error) = client.cancel_setup().await {
                        return print_client_error(error);
                    }
                    ui::warn("Join setup canceled.");
                    return exit_codes::EXIT_SUCCESS;
                }
                Err(error) => {
                    ui::error(&error);
                    return exit_codes::EXIT_ERROR;
                }
            }
        } else if should_prompt_for_join_passphrase(&state) {
            finish_spinner(&mut spinner);
            if let Some(message) = join_retry_message(&state) {
                ui::warn(message);
            }
            let passphrase: String = match ui::password("Space passphrase") {
                Ok(p) if p.trim().is_empty() => {
                    ui::error("Passphrase cannot be empty");
                    return exit_codes::EXIT_ERROR;
                }
                Ok(p) => p,
                Err(error) => {
                    ui::error(&error);
                    return exit_codes::EXIT_ERROR;
                }
            };
            spinner = Some(ui::spinner("Verifying passphrase…"));
            if let Err(error) = client.submit_setup_passphrase(passphrase).await {
                finish_spinner(&mut spinner);
                return print_client_error(error);
            }
        } else if state.next_step_hint == "idle" && submitted_peer_request {
            finish_spinner(&mut spinner);
            ui::error("Setup returned to idle before completion");
            return exit_codes::EXIT_ERROR;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ── Status & Reset (non-interactive) ────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetupStatusOutput {
    state: Value,
    session_id: Option<String>,
    next_step_hint: String,
    profile: String,
    clipboard_mode: String,
    device_name: String,
    peer_id: String,
}

impl fmt::Display for SetupStatusOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "state: {}",
            setup_state_variant(&self.state).unwrap_or("unknown")
        )?;
        writeln!(
            f,
            "sessionId: {}",
            self.session_id.as_deref().unwrap_or("-")
        )?;
        writeln!(f, "nextStepHint: {}", self.next_step_hint)?;
        writeln!(f, "profile: {}", self.profile)?;
        writeln!(f, "clipboardMode: {}", self.clipboard_mode)?;
        writeln!(f, "deviceName: {}", self.device_name)?;
        write!(f, "peerId: {}", self.peer_id)
    }
}

pub async fn run_status(json: bool, _verbose: bool) -> i32 {
    let client = match DaemonHttpClient::new() {
        Ok(client) => client,
        Err(error) => return print_client_error(error),
    };

    let state = match client.get_setup_state().await {
        Ok(state) => state,
        Err(error) => return print_client_error(error),
    };

    let output_value = SetupStatusOutput::from(state);
    if let Err(error) = output::print_result(&output_value, json) {
        eprintln!("Error: {error}");
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}

pub async fn run_reset(json: bool, _verbose: bool) -> i32 {
    if json {
        eprintln!("Error: `--json` is not supported with `setup reset`");
        return exit_codes::EXIT_ERROR;
    }

    if let Err(error) = ensure_local_daemon_running().await {
        return print_local_daemon_error(error);
    }

    let client = match DaemonHttpClient::new() {
        Ok(client) => client,
        Err(error) => return print_client_error(error),
    };

    let response = match client.reset_setup().await {
        Ok(response) => response,
        Err(error) => return print_client_error(error),
    };

    ui::success(&render_reset_output(
        &response.profile,
        response.daemon_kept_running,
    ));

    exit_codes::EXIT_SUCCESS
}

// ── From impl ───────────────────────────────────────────────────────

impl From<SetupStateResponse> for SetupStatusOutput {
    fn from(value: SetupStateResponse) -> Self {
        Self {
            state: value.state,
            session_id: value.session_id,
            next_step_hint: value.next_step_hint,
            profile: value.profile,
            clipboard_mode: value.clipboard_mode,
            device_name: value.device_name,
            peer_id: value.peer_id,
        }
    }
}

// ── Prompt helpers ──────────────────────────────────────────────────

enum HostDecision {
    Accept,
    Reject,
}

fn stdin_is_terminal() -> bool {
    io::stdin().is_terminal()
}

fn print_identity_banner(state: &SetupStateResponse) {
    ui::identity_banner(
        &state.profile,
        &state.clipboard_mode,
        &state.device_name,
        &state.peer_id,
    );
}

fn prompt_new_space_passphrase() -> Result<String, String> {
    ui::bar();
    ui::password_with_confirm("New space passphrase", "Confirm passphrase")
}

fn prompt_host_decision(state: &SetupStateResponse) -> Result<HostDecision, String> {
    let peer_name = state
        .selected_peer_name
        .as_deref()
        .or(state.selected_peer_id.as_deref())
        .unwrap_or("unknown peer");

    ui::step(&format!("Join request from {}", style(peer_name).bold()));
    if let Some(short_code) = setup_state_short_code(&state.state) {
        ui::verification_code(short_code);
    }

    let accepted = ui::confirm("Accept this peer?", true)?;
    if accepted {
        Ok(HostDecision::Accept)
    } else {
        Ok(HostDecision::Reject)
    }
}

fn prompt_host_verification(state: &SetupStateResponse) -> Result<bool, String> {
    let peer_name = state
        .selected_peer_name
        .as_deref()
        .or(state.selected_peer_id.as_deref())
        .unwrap_or("selected peer");

    ui::step(&format!(
        "Confirm peer trust for {}",
        style(peer_name).bold()
    ));
    if let Some(short_code) = setup_state_short_code(&state.state) {
        ui::verification_code(short_code);
    }

    ui::confirm("Do the verification codes match?", true)
}

fn prompt_join_peer_confirmation(state: &SetupStateResponse) -> Result<bool, String> {
    let peer_name = state
        .selected_peer_name
        .as_deref()
        .or(state.selected_peer_id.as_deref())
        .unwrap_or("selected peer");

    ui::step(&format!(
        "Confirm peer trust for {}",
        style(peer_name).bold()
    ));
    if let Some(short_code) = setup_state_short_code(&state.state) {
        ui::verification_code(short_code);
    }

    ui::confirm("Do the verification codes match?", true)
}

fn prompt_for_peer_selection(peers: &[PeerSnapshotDto]) -> Result<Option<String>, String> {
    let items: Vec<String> = peers
        .iter()
        .map(|peer| {
            let name = peer.device_name.as_deref().unwrap_or("unknown device");
            format!("{name} ({})", truncate_id(&peer.peer_id))
        })
        .collect();

    let mut all_items = items;
    all_items.push(style("Cancel").dim().to_string());

    ui::step("Select a peer to join");

    let chosen = ui::select("Discovered peers", &all_items)?;

    if chosen == all_items.len() - 1 {
        return Ok(None);
    }

    Ok(Some(peers[chosen].peer_id.clone()))
}

// ── Spinner management ──────────────────────────────────────────────

fn finish_spinner(spinner: &mut Option<indicatif::ProgressBar>) {
    if let Some(pb) = spinner.take() {
        pb.finish_and_clear();
    }
}

// ── State inspection helpers ────────────────────────────────────────

pub(crate) fn should_enable_host_pairing_presence(
    state: &SetupStateResponse,
    already_enabled: bool,
) -> bool {
    !already_enabled && state.next_step_hint == "completed"
}

fn state_signature(state: &SetupStateResponse) -> String {
    format!(
        "{}:{}:{}:{}",
        state.next_step_hint,
        state.session_id.as_deref().unwrap_or("-"),
        setup_state_variant(&state.state).unwrap_or("unknown"),
        setup_state_error_code(&state.state).unwrap_or("-")
    )
}

pub(crate) fn should_prompt_for_join_passphrase(state: &SetupStateResponse) -> bool {
    state.next_step_hint == "join-enter-passphrase"
        || matches!(
            setup_state_variant(&state.state),
            Some("JoinSpaceInputPassphrase")
        )
}

pub(crate) fn should_prompt_for_join_peer_confirmation(state: &SetupStateResponse) -> bool {
    matches!(
        setup_state_variant(&state.state),
        Some("JoinSpaceConfirmPeer")
    )
}

pub(crate) fn should_prompt_for_host_verification(state: &SetupStateResponse) -> bool {
    state.has_completed
        && matches!(
            setup_state_variant(&state.state),
            Some("JoinSpaceConfirmPeer")
        )
}

pub(crate) fn host_flow_completed(state: &SetupStateResponse, handled_peer_request: bool) -> bool {
    handled_peer_request
        && state.has_completed
        && state.next_step_hint == "completed"
        && state.session_id.is_none()
}

fn setup_state_variant(state: &Value) -> Option<&str> {
    match state {
        Value::String(value) => Some(value.as_str()),
        Value::Object(map) if map.len() == 1 => map.keys().next().map(String::as_str),
        _ => None,
    }
}

fn setup_state_error_code(state: &Value) -> Option<&str> {
    let variant = setup_state_variant(state)?;
    let payload = match state {
        Value::Object(map) => map.get(variant)?,
        _ => return None,
    };
    payload.get("error")?.as_str()
}

fn setup_state_short_code(state: &Value) -> Option<&str> {
    let payload = match state {
        Value::Object(map) => map.get("JoinSpaceConfirmPeer")?,
        _ => return None,
    };
    payload.get("short_code")?.as_str()
}

pub(crate) fn join_retry_message(state: &SetupStateResponse) -> Option<&'static str> {
    if setup_state_error_code(&state.state) == Some("PassphraseInvalidOrMismatch") {
        Some("Passphrase rejected; retrying current join session")
    } else {
        None
    }
}

pub(crate) fn render_reset_output(profile: &str, daemon_kept_running: bool) -> String {
    let mut lines = vec![format!("Reset complete for profile {profile}")];
    if daemon_kept_running {
        lines.push("Daemon kept running".to_string());
    }
    lines.join("\n")
}

fn filter_joinable_peers(peers: Vec<PeerSnapshotDto>) -> Vec<PeerSnapshotDto> {
    let mut peers: Vec<_> = peers.into_iter().filter(|peer| !peer.is_paired).collect();
    peers.sort_by(|left, right| {
        left.device_name
            .as_deref()
            .unwrap_or(left.peer_id.as_str())
            .cmp(
                right
                    .device_name
                    .as_deref()
                    .unwrap_or(right.peer_id.as_str()),
            )
    });
    peers
}

fn truncate_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}…", &id[..12])
    } else {
        id.to_string()
    }
}

async fn disable_host_pairing_presence(
    client: &DaemonHttpClient,
    host_pairing_presence_enabled: &mut bool,
) -> Result<(), DaemonClientError> {
    if !*host_pairing_presence_enabled {
        return Ok(());
    }

    client.set_pairing_gui_lease(false).await?;
    *host_pairing_presence_enabled = false;
    Ok(())
}

fn print_local_daemon_error(error: LocalDaemonError) -> i32 {
    ui::error(&format!("{error}"));
    exit_codes::EXIT_DAEMON_UNREACHABLE
}

fn print_client_error(error: DaemonClientError) -> i32 {
    match error {
        DaemonClientError::Unreachable(_) => {
            ui::error("Daemon unreachable (is uniclipboard-daemon running?)");
            exit_codes::EXIT_DAEMON_UNREACHABLE
        }
        DaemonClientError::Unauthorized => {
            ui::error("Daemon rejected request: invalid or missing auth token");
            exit_codes::EXIT_ERROR
        }
        DaemonClientError::Initialization(_)
        | DaemonClientError::UnexpectedStatus { .. }
        | DaemonClientError::InvalidResponse(_) => {
            ui::error(&format!("{error}"));
            exit_codes::EXIT_ERROR
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identity_banner_contains_fixed_fields() {
        let state = SetupStateResponse {
            state: Value::String("Welcome".to_string()),
            session_id: Some("session-1".to_string()),
            next_step_hint: "idle".to_string(),
            profile: "peerA".to_string(),
            clipboard_mode: "full".to_string(),
            device_name: "Peer A".to_string(),
            peer_id: "peer-a".to_string(),
            selected_peer_id: None,
            selected_peer_name: None,
            has_completed: false,
        };

        // Just verify the output doesn't panic.
        let output = SetupStatusOutput::from(state);
        let rendered = format!("{output}");
        assert!(rendered.contains("peerA"));
        assert!(rendered.contains("full"));
        assert!(rendered.contains("Peer A"));
        assert!(rendered.contains("peer-a"));
    }

    #[test]
    fn setup_status_output_serializes_camel_case_keys() {
        let output = SetupStatusOutput {
            state: json!({"Completed": null}),
            session_id: Some("session-1".to_string()),
            next_step_hint: "completed".to_string(),
            profile: "peerA".to_string(),
            clipboard_mode: "full".to_string(),
            device_name: "Peer A".to_string(),
            peer_id: "peer-a".to_string(),
        };

        let value = serde_json::to_value(output).expect("status output should serialize");
        assert_eq!(value["sessionId"], "session-1");
        assert_eq!(value["nextStepHint"], "completed");
        assert_eq!(value["clipboardMode"], "full");
        assert_eq!(value["deviceName"], "Peer A");
        assert_eq!(value["peerId"], "peer-a");
        assert!(value.get("session_id").is_none());
    }

    #[test]
    fn detects_setup_variant_and_error_code() {
        let state = json!({
            "JoinSpaceInputPassphrase": {
                "error": "PassphraseInvalidOrMismatch"
            }
        });

        assert_eq!(
            setup_state_variant(&state),
            Some("JoinSpaceInputPassphrase")
        );
        assert_eq!(
            setup_state_error_code(&state),
            Some("PassphraseInvalidOrMismatch")
        );
    }

    #[test]
    fn filters_out_already_paired_peers_before_selection() {
        let peers = vec![
            PeerSnapshotDto {
                peer_id: "peer-b".to_string(),
                device_name: Some("Peer B".to_string()),
                addresses: vec![],
                is_paired: true,
                connected: true,
                pairing_state: "Paired".to_string(),
            },
            PeerSnapshotDto {
                peer_id: "peer-a".to_string(),
                device_name: Some("Peer A".to_string()),
                addresses: vec![],
                is_paired: false,
                connected: true,
                pairing_state: "Discovered".to_string(),
            },
        ];

        let filtered = filter_joinable_peers(peers);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].peer_id, "peer-a");
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
    fn truncate_id_short_ids_unchanged() {
        assert_eq!(truncate_id("short"), "short");
    }

    #[test]
    fn truncate_id_long_ids_truncated() {
        let long = "abcdefghijklmnopqrstuvwxyz";
        let result = truncate_id(long);
        assert!(result.ends_with('…'));
        assert_eq!(result.len(), "abcdefghijkl".len() + '…'.len_utf8());
    }
}
