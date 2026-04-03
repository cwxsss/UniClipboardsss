//! Interactive setup commands over daemon-owned setup state.

use std::fmt;
use std::io::{self, IsTerminal};
use std::time::Duration;

use console::style;
use serde::Serialize;
use serde_json::Value;
use uc_app::usecases::CoreUseCases;
use uc_core::security::model::Passphrase;
use uc_core::security::state::EncryptionState;
use uc_daemon::api::dto::setup::SetupStateResponseDto;
use uc_daemon::api::types::{PeerSnapshotDto, SetupStateResponse};
use uc_daemon_client::{DaemonClientContext, DaemonPairingClient};

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
        1 => run_connect(json, verbose).await,
        _ => unreachable!(),
    }
}

// ── New Space flow (create encrypted space only, no pairing) ────────

/// Returns `Ok(())` if encryption state allows new-space initialization,
/// or `Err(exit_code)` if the operation should be rejected.
///
/// Uses a whitelist approach: only `Uninitialized` is allowed.
/// All other states (Initializing, Initialized, Error, etc.) are rejected.
pub fn new_space_encryption_guard(state: EncryptionState) -> Result<(), i32> {
    match state {
        EncryptionState::Uninitialized => Ok(()),
        _ => Err(exit_codes::EXIT_ERROR),
    }
}

async fn run_new_space() -> i32 {
    // 1. Build CLI runtime directly (no daemon needed for encryption init)
    let runtime = match uc_bootstrap::build_cli_runtime(Some(uc_observability::LogProfile::Cli)) {
        Ok(r) => r,
        Err(e) => {
            ui::error(&format!("Failed to initialize: {e}"));
            return exit_codes::EXIT_ERROR;
        }
    };

    // 2. Check encryption state — reject if already initialized
    let state = match runtime.encryption_state().await {
        Ok(s) => s,
        Err(e) => {
            ui::error(&format!("Failed to check encryption state: {e}"));
            return exit_codes::EXIT_ERROR;
        }
    };

    if let Err(code) = new_space_encryption_guard(state) {
        ui::error("Space already initialized.");
        ui::info(
            "Hint",
            "run `uniclipboard setup` and select 'Create new Space' to initialize your space first",
        );
        return code;
    }

    // 3. Prompt for passphrase
    let passphrase_str = match prompt_new_space_passphrase() {
        Ok(p) => p,
        Err(e) => {
            ui::error(&e);
            return exit_codes::EXIT_ERROR;
        }
    };

    // 4. Initialize encryption locally (no daemon involved)
    let spinner = ui::spinner("Creating encrypted space...");
    let uc = CoreUseCases::new(&runtime);
    match uc
        .initialize_encryption()
        .execute(Passphrase(passphrase_str))
        .await
    {
        Ok(()) => {
            ui::spinner_finish_success(&spinner, "Encrypted space created");
        }
        Err(e) => {
            ui::spinner_finish_error(&spinner, &format!("Failed to create space: {e}"));
            return exit_codes::EXIT_ERROR;
        }
    }

    // 5. Persist setup completion so daemon/GUI recognise setup is done
    if let Err(e) = uc.mark_setup_complete().execute().await {
        ui::error(&format!("Failed to persist setup status: {e}"));
        return exit_codes::EXIT_ERROR;
    }

    // 6. Success
    ui::bar();
    ui::end("Setup complete! Your space is ready.");
    exit_codes::EXIT_SUCCESS
}

// ── Host flow ───────────────────────────────────────────────────────

pub async fn run_pair(json: bool, _verbose: bool) -> i32 {
    if json {
        eprintln!("Error: `--json` is only supported with `setup status`");
        return exit_codes::EXIT_ERROR;
    }
    if !stdin_is_terminal() {
        eprintln!("Error: `setup pair` requires an interactive terminal");
        return exit_codes::EXIT_ERROR;
    }

    if let Err(error) = ensure_local_daemon_running().await {
        return print_local_daemon_error(error);
    }

    let ctx = match DaemonClientContext::from_env() {
        Ok(ctx) => ctx,
        Err(error) => {
            ui::error(&format!("Failed to connect to daemon: {error}"));
            return exit_codes::EXIT_DAEMON_UNREACHABLE;
        }
    };
    let setup_client = ctx.setup_client();
    let pairing_client = ctx.pairing_client();

    let initial_state: SetupStateResponseDto = match setup_client.get_setup_state().await {
        Ok(state) => state.data,
        Err(error) => return print_anyhow_error(error),
    };

    ui::step("Device identity");
    print_identity_banner(&initial_state);

    // Guard: space must already be initialized before entering pairing mode
    if !initial_state.has_completed {
        ui::error("Space is not initialized.");
        ui::info(
            "Hint",
            "run `uniclipboard setup` and select 'Create new Space' to initialize your space first",
        );
        return exit_codes::EXIT_ERROR;
    }

    let mut handled_peer_request = false;
    let mut submitted_host_decision_session: Option<String> = None;
    let mut handled_host_verification = false;
    let mut submitted_host_verification_session: Option<String> = None;
    let mut host_pairing_presence_enabled = false;
    let mut last_host_lease_refresh = std::time::Instant::now();
    let mut spinner: Option<indicatif::ProgressBar> = None;

    loop {
        let state: SetupStateResponseDto = match setup_client.get_setup_state().await {
            Ok(state) => state.data,
            Err(error) => {
                finish_spinner(&mut spinner);
                return print_anyhow_error(error);
            }
        };

        if !matches!(state.next_step_hint.as_str(), "host-confirm-peer")
            && !matches!(
                setup_state_variant(&state.state),
                Some("JoinSpaceConfirmPeer")
            )
        {
            submitted_host_decision_session = None;
        }

        if state.next_step_hint != "host-confirm-peer"
            || !matches!(
                setup_state_variant(&state.state),
                Some("JoinSpaceConfirmPeer")
            )
        {
            submitted_host_verification_session = None;
        }

        if should_prompt_host_verification(&state, submitted_host_verification_session.as_deref()) {
            finish_spinner(&mut spinner);
            let session_id = match state.session_id.clone() {
                Some(session_id) => session_id,
                None => {
                    ui::error("Missing pairing session id for host verification");
                    return exit_codes::EXIT_ERROR;
                }
            };
            match prompt_host_verification(&state) {
                Ok(true) => {
                    if let Err(error) = setup_client.confirm_peer_trust().await {
                        return print_anyhow_error(error);
                    }
                    handled_host_verification = true;
                    submitted_host_verification_session = Some(session_id);
                }
                Ok(false) => {
                    if let Err(error) = setup_client.cancel_setup().await {
                        return print_anyhow_error(error);
                    }
                    ui::warn("Host pairing canceled.");
                    return exit_codes::EXIT_SUCCESS;
                }
                Err(error) => {
                    ui::error(&error);
                    return exit_codes::EXIT_ERROR;
                }
            }
        } else if should_prompt_host_decision(&state, submitted_host_decision_session.as_deref()) {
            finish_spinner(&mut spinner);
            handled_peer_request = true;
            let session_id = state.session_id.clone();
            match prompt_host_decision(&state) {
                Ok(HostDecision::Accept) => {
                    let accept_result = setup_client.confirm_peer_trust().await;
                    if let Err(error) = accept_result {
                        return print_anyhow_error(error);
                    }
                    submitted_host_decision_session = session_id;
                }
                Ok(HostDecision::Reject) => {
                    if let Err(error) = setup_client.cancel_setup().await {
                        return print_anyhow_error(error);
                    }
                    let _ = disable_host_pairing_presence(
                        &pairing_client,
                        &mut host_pairing_presence_enabled,
                    )
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
            if !host_pairing_presence_enabled {
                if let Err(error) = pairing_client.register_gui_participant(true, None).await {
                    finish_spinner(&mut spinner);
                    return print_anyhow_error(error);
                }
                host_pairing_presence_enabled = true;
                last_host_lease_refresh = std::time::Instant::now();
            } else if host_pairing_presence_enabled
                && last_host_lease_refresh.elapsed() >= HOST_LEASE_REFRESH_INTERVAL
            {
                if let Err(error) = pairing_client.register_gui_participant(true, None).await {
                    finish_spinner(&mut spinner);
                    return print_anyhow_error(error);
                }
                last_host_lease_refresh = std::time::Instant::now();
            }
            if spinner.is_none() {
                spinner = Some(ui::spinner("Host ready. Waiting for a join request…"));
            }
        } else if should_complete_host_flow(&state, handled_peer_request, handled_host_verification)
        {
            finish_spinner(&mut spinner);
            let _ =
                disable_host_pairing_presence(&pairing_client, &mut host_pairing_presence_enabled)
                    .await;
            ui::success("Setup host flow completed!");
            return exit_codes::EXIT_SUCCESS;
        } else if state.next_step_hint == "idle" && handled_peer_request {
            finish_spinner(&mut spinner);
            let _ =
                disable_host_pairing_presence(&pairing_client, &mut host_pairing_presence_enabled)
                    .await;
            ui::end("Host setup returned to idle.");
            return exit_codes::EXIT_SUCCESS;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

// ── Join flow ───────────────────────────────────────────────────────

pub async fn run_connect(json: bool, _verbose: bool) -> i32 {
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

    let ctx = match DaemonClientContext::from_env() {
        Ok(ctx) => ctx,
        Err(error) => {
            ui::error(&format!("Failed to connect to daemon: {error}"));
            return exit_codes::EXIT_DAEMON_UNREACHABLE;
        }
    };
    let setup_client = ctx.setup_client();
    let query_client = ctx.query_client();

    let initial_state: SetupStateResponseDto = match setup_client.get_setup_state().await {
        Ok(state) => state.data,
        Err(error) => return print_anyhow_error(error),
    };

    ui::step("Device identity");
    print_identity_banner(&initial_state);

    if let Err(error) = setup_client.start_join_space().await {
        return print_anyhow_error(error);
    }

    let mut submitted_peer_request = false;
    let mut spinner: Option<indicatif::ProgressBar> = None;

    loop {
        let state: SetupStateResponseDto = match setup_client.get_setup_state().await {
            Ok(state) => state.data,
            Err(error) => {
                finish_spinner(&mut spinner);
                return print_anyhow_error(error);
            }
        };

        if state.has_completed || state.next_step_hint == "completed" {
            finish_spinner(&mut spinner);
            ui::success("Setup join flow completed!");
            return exit_codes::EXIT_SUCCESS;
        }

        if state.next_step_hint == "join-select-peer" {
            let peers = match query_client.get_peers().await {
                Ok(peers) => filter_joinable_peers(peers),
                Err(error) => {
                    finish_spinner(&mut spinner);
                    return print_anyhow_error(error);
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
                        if let Err(error) = setup_client.select_device(peer_id).await {
                            finish_spinner(&mut spinner);
                            return print_anyhow_error(error);
                        }
                    }
                    Ok(None) => {
                        if let Err(error) = setup_client.cancel_setup().await {
                            return print_anyhow_error(error);
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
        } else if matches!(
            setup_state_variant(&state.state),
            Some("JoinSpaceConfirmPeer")
        ) {
            finish_spinner(&mut spinner);
            match prompt_join_peer_confirmation(&state) {
                Ok(true) => {
                    if let Err(error) = setup_client.confirm_peer_trust().await {
                        return print_anyhow_error(error);
                    }
                }
                Ok(false) => {
                    if let Err(error) = setup_client.cancel_setup().await {
                        return print_anyhow_error(error);
                    }
                    ui::warn("Join setup canceled.");
                    return exit_codes::EXIT_SUCCESS;
                }
                Err(error) => {
                    ui::error(&error);
                    return exit_codes::EXIT_ERROR;
                }
            }
        } else if state.next_step_hint == "join-enter-passphrase"
            || matches!(
                setup_state_variant(&state.state),
                Some("JoinSpaceInputPassphrase")
            )
        {
            finish_spinner(&mut spinner);
            if setup_state_error_code(&state.state) == Some("PassphraseInvalidOrMismatch") {
                ui::warn("Passphrase rejected; retrying current join session");
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
            if let Err(error) = setup_client.verify_passphrase(passphrase).await {
                finish_spinner(&mut spinner);
                return print_anyhow_error(error);
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
    let ctx = match DaemonClientContext::from_env() {
        Ok(ctx) => ctx,
        Err(error) => {
            ui::error(&format!("Failed to connect to daemon: {error}"));
            return exit_codes::EXIT_DAEMON_UNREACHABLE;
        }
    };
    let setup_client = ctx.setup_client();

    let state_dto = match setup_client.get_setup_state().await {
        Ok(state) => state.data,
        Err(error) => return print_anyhow_error(error),
    };

    let output_value = SetupStatusOutput::from(state_dto);
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

    let ctx = match DaemonClientContext::from_env() {
        Ok(ctx) => ctx,
        Err(error) => {
            ui::error(&format!("Failed to connect to daemon: {error}"));
            return exit_codes::EXIT_DAEMON_UNREACHABLE;
        }
    };

    let response = match ctx.setup_client().reset_setup().await {
        Ok(response) => response,
        Err(error) => return print_anyhow_error(error),
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

impl From<SetupStateResponseDto> for SetupStatusOutput {
    fn from(value: SetupStateResponseDto) -> Self {
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

fn print_identity_banner(state: &SetupStateResponseDto) {
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

fn prompt_host_decision(state: &SetupStateResponseDto) -> Result<HostDecision, String> {
    let peer_name = format_selected_peer_label(state).unwrap_or_else(|| "unknown peer".to_string());
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

pub(crate) fn should_prompt_host_decision(
    state: &SetupStateResponseDto,
    submitted_session_id: Option<&str>,
) -> bool {
    if state.next_step_hint != "host-confirm-peer" {
        return false;
    }

    if matches!(
        setup_state_variant(&state.state),
        Some("JoinSpaceConfirmPeer")
    ) {
        return false;
    }

    state.session_id.as_deref() != submitted_session_id
}

fn should_prompt_host_verification(
    state: &SetupStateResponseDto,
    submitted_session_id: Option<&str>,
) -> bool {
    if state.next_step_hint != "host-confirm-peer" {
        return false;
    }

    if !matches!(
        setup_state_variant(&state.state),
        Some("JoinSpaceConfirmPeer")
    ) {
        return false;
    }

    state.session_id.as_deref() != submitted_session_id
}

fn prompt_host_verification(state: &SetupStateResponseDto) -> Result<bool, String> {
    let peer_name =
        format_selected_peer_label(state).unwrap_or_else(|| "selected peer".to_string());

    ui::step(&format!(
        "Confirm peer trust for {}",
        style(peer_name).bold()
    ));
    if let Some(short_code) = setup_state_short_code(&state.state) {
        ui::verification_code(short_code);
    }

    ui::confirm("Do the verification codes match?", true)
}

fn prompt_join_peer_confirmation(state: &SetupStateResponseDto) -> Result<bool, String> {
    let peer_name =
        format_selected_peer_label(state).unwrap_or_else(|| "selected peer".to_string());

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

pub(crate) fn setup_state_variant(state: &Value) -> Option<&str> {
    match state {
        Value::String(value) => Some(value.as_str()),
        Value::Object(map) if map.len() == 1 => map.keys().next().map(String::as_str),
        _ => None,
    }
}

pub(crate) fn setup_state_error_code(state: &Value) -> Option<&str> {
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

pub(crate) fn should_complete_host_flow(
    state: &SetupStateResponseDto,
    handled_peer_request: bool,
    handled_host_verification: bool,
) -> bool {
    handled_peer_request
        && handled_host_verification
        && state.has_completed
        && state.next_step_hint == "completed"
        && state.session_id.is_none()
}

pub(crate) fn format_selected_peer_label(state: &SetupStateResponseDto) -> Option<String> {
    let peer_id = state.selected_peer_id.as_deref();
    let peer_name = state.selected_peer_name.as_deref().map(str::trim);

    match (peer_name, peer_id) {
        (Some(name), Some(peer_id)) if !name.is_empty() => {
            Some(format!("{name} ({})", format_peer_id_suffix(peer_id)))
        }
        (Some(name), None) if !name.is_empty() => Some(name.to_string()),
        (_, Some(peer_id)) => Some(format_peer_id_suffix(peer_id)),
        _ => None,
    }
}

fn format_peer_id_suffix(peer_id: &str) -> String {
    if peer_id.len() <= 8 {
        peer_id.to_string()
    } else {
        peer_id[peer_id.len() - 8..].to_string()
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
    client: &DaemonPairingClient,
    host_pairing_presence_enabled: &mut bool,
) -> Result<(), anyhow::Error> {
    if !*host_pairing_presence_enabled {
        return Ok(());
    }

    client.register_gui_participant(false, None).await?;
    *host_pairing_presence_enabled = false;
    Ok(())
}

fn print_local_daemon_error(error: LocalDaemonError) -> i32 {
    ui::error(&format!("{error}"));
    exit_codes::EXIT_DAEMON_UNREACHABLE
}

fn print_anyhow_error(error: anyhow::Error) -> i32 {
    ui::error(&format!("{error}"));
    exit_codes::EXIT_ERROR
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

    #[test]
    fn format_selected_peer_label_uses_peer_suffix_when_available() {
        let state = SetupStateResponse {
            state: json!("Completed"),
            session_id: Some("session-1".to_string()),
            next_step_hint: "host-confirm-peer".to_string(),
            profile: "peerA".to_string(),
            clipboard_mode: "full".to_string(),
            device_name: "Peer A".to_string(),
            peer_id: "peer-a".to_string(),
            selected_peer_id: Some("12D3KooWABCDEFGH".to_string()),
            selected_peer_name: Some("Peer B".to_string()),
            has_completed: true,
        };

        assert_eq!(
            format_selected_peer_label(&state.into()),
            Some("Peer B (ABCDEFGH)".to_string())
        );
    }
}
