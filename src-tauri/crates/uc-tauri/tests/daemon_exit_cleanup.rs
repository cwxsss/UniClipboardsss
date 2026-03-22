//! Canonical regression target for Phase 46.6 daemon exit cleanup work.
//!
//! Run with:
//! `cd src-tauri && cargo test -p uc-tauri --test daemon_exit_cleanup -- --test-threads=1`
//! `cd src-tauri && cargo test -p uc-tauri --test daemon_bootstrap_contract -- --test-threads=1`

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use uc_daemon::api::types::HealthResponse;
use uc_daemon::DAEMON_API_REVISION;
use uc_tauri::bootstrap::run::ProbeOutcome;
use uc_tauri::bootstrap::{GuiOwnedDaemonState, SpawnReason};

const EXIT_CLEANUP_COMMAND: &str =
    "cargo test -p uc-tauri --test daemon_exit_cleanup -- --test-threads=1";
const BOOTSTRAP_CONTRACT_COMMAND: &str =
    "cargo test -p uc-tauri --test daemon_bootstrap_contract -- --test-threads=1";
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const CLEANUP_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone)]
struct ExitCleanupFixture {
    probe_outcome: ProbeOutcome,
    spawn_reason: Option<SpawnReason>,
}

impl ExitCleanupFixture {
    fn spawned_gui_daemon() -> Self {
        Self {
            probe_outcome: ProbeOutcome::Absent,
            spawn_reason: Some(SpawnReason::Absent),
        }
    }

    fn compatible_existing_daemon() -> Self {
        Self {
            probe_outcome: ProbeOutcome::Compatible(compatible_health()),
            spawn_reason: None,
        }
    }

    fn replacement_owned_daemon() -> Self {
        Self {
            probe_outcome: ProbeOutcome::Incompatible {
                details: "stale daemon build".to_string(),
                observed_package_version: Some("0.0.0-stale".to_string()),
                observed_api_revision: Some("legacy-v0".to_string()),
            },
            spawn_reason: Some(SpawnReason::Replacement),
        }
    }

    fn canonical_command(&self) -> &'static str {
        EXIT_CLEANUP_COMMAND
    }

    fn bootstrap_contract_command(&self) -> &'static str {
        BOOTSTRAP_CONTRACT_COMMAND
    }

    fn records_owned_child(&self) -> bool {
        self.spawn_reason.is_some()
    }

    fn install_owned_child(&self, state: &GuiOwnedDaemonState) -> Option<u32> {
        let spawn_reason = self.spawn_reason?;
        let child = spawn_sleeping_child();
        let pid = child.id();
        state.record_spawned(child, spawn_reason);
        Some(pid)
    }
}

fn compatible_health() -> HealthResponse {
    HealthResponse {
        status: "ok".to_string(),
        package_version: env!("CARGO_PKG_VERSION").to_string(),
        api_revision: DAEMON_API_REVISION.to_string(),
    }
}

fn spawn_sleeping_child() -> Child {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("sh");
        command.arg("-c").arg("sleep 60");
        command
    };

    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "ping -n 60 127.0.0.1 > NUL"]);
        command
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn long-lived test child")
}

fn assert_canonical_commands(fixture: &ExitCleanupFixture) {
    assert_eq!(fixture.canonical_command(), EXIT_CLEANUP_COMMAND);
    assert_eq!(
        fixture.bootstrap_contract_command(),
        BOOTSTRAP_CONTRACT_COMMAND
    );
}

fn main_rs_source() -> String {
    let main_rs = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/main.rs")
        .canonicalize()
        .expect("resolve src/main.rs path");
    fs::read_to_string(main_rs).expect("read src/main.rs")
}

fn close_requested_block(source: &str) -> &str {
    let start = source
        .find("if let tauri::WindowEvent::CloseRequested { api, .. } = event {")
        .expect("close request block should exist");
    let end = source[start..]
        .find("info!(\"Main window hidden to tray\");")
        .map(|offset| start + offset)
        .expect("close request block should log tray hide");
    &source[start..end]
}

#[tokio::test]
async fn daemon_exit_cleanup_terminates_spawned_gui_daemon() {
    let fixture = ExitCleanupFixture::spawned_gui_daemon();
    let state = GuiOwnedDaemonState::default();

    assert_canonical_commands(&fixture);
    assert!(fixture.records_owned_child());
    assert!(matches!(fixture.probe_outcome, ProbeOutcome::Absent));

    let pid = fixture
        .install_owned_child(&state)
        .expect("spawned GUI daemon should record owned child");

    let cleaned = state
        .shutdown_owned_daemon(CLEANUP_TIMEOUT, CLEANUP_POLL_INTERVAL)
        .await
        .expect("spawned GUI daemon cleanup should succeed");

    assert!(cleaned);
    assert_eq!(state.snapshot_pid(), None);
    assert!(process_is_not_running(pid));
}

#[tokio::test]
async fn daemon_exit_cleanup_skips_when_gui_did_not_spawn_daemon() {
    let fixture = ExitCleanupFixture::compatible_existing_daemon();
    let state = GuiOwnedDaemonState::default();

    assert_canonical_commands(&fixture);
    assert!(!fixture.records_owned_child());
    assert!(matches!(fixture.probe_outcome, ProbeOutcome::Compatible(_)));

    let cleaned = state
        .shutdown_owned_daemon(CLEANUP_TIMEOUT, CLEANUP_POLL_INTERVAL)
        .await
        .expect("compatible existing daemon should be a cleanup no-op");

    assert!(!cleaned);
    assert_eq!(state.snapshot_pid(), None);
}

#[tokio::test]
async fn daemon_exit_cleanup_clears_owned_pid_after_success() {
    let fixture = ExitCleanupFixture::spawned_gui_daemon();
    let state = GuiOwnedDaemonState::default();

    let pid = fixture
        .install_owned_child(&state)
        .expect("spawned GUI daemon should record owned child");
    assert_eq!(state.snapshot_pid(), Some(pid));

    state
        .shutdown_owned_daemon(CLEANUP_TIMEOUT, CLEANUP_POLL_INTERVAL)
        .await
        .expect("cleanup should clear owned pid after success");

    assert_eq!(state.snapshot_pid(), None);
}

#[tokio::test]
async fn daemon_exit_cleanup_is_idempotent_across_repeated_exit_requests() {
    let fixture = ExitCleanupFixture::spawned_gui_daemon();
    let state = GuiOwnedDaemonState::default();

    fixture.install_owned_child(&state);

    assert!(state.begin_exit_cleanup());
    assert!(!state.begin_exit_cleanup());

    let cleaned = state
        .shutdown_owned_daemon(CLEANUP_TIMEOUT, CLEANUP_POLL_INTERVAL)
        .await
        .expect("first exit cleanup should succeed");
    assert!(cleaned);

    state.finish_exit_cleanup();
    assert!(state.begin_exit_cleanup());

    let second_cleaned = state
        .shutdown_owned_daemon(CLEANUP_TIMEOUT, CLEANUP_POLL_INTERVAL)
        .await
        .expect("second exit cleanup should be a no-op");
    assert!(!second_cleaned);

    state.finish_exit_cleanup();
}

#[test]
fn main_window_close_request_hides_to_tray_without_daemon_cleanup() {
    let source = main_rs_source();
    let close_block = close_requested_block(&source);

    assert!(source.contains("WindowEvent::CloseRequested"));
    assert!(close_block.contains("api.prevent_close();"));
    assert!(close_block.contains("let _ = window.hide();"));
    assert!(!close_block.contains("shutdown_owned_daemon"));
    assert!(!close_block.contains("terminate_local_daemon_pid"));
}

#[tokio::test]
async fn daemon_exit_cleanup_terminates_replacement_owned_daemon() {
    let fixture = ExitCleanupFixture::replacement_owned_daemon();
    let state = GuiOwnedDaemonState::default();

    assert_canonical_commands(&fixture);
    assert!(fixture.records_owned_child());
    assert!(matches!(
        fixture.probe_outcome,
        ProbeOutcome::Incompatible { .. }
    ));

    let pid = fixture
        .install_owned_child(&state)
        .expect("replacement path should record owned child");

    let cleaned = state
        .shutdown_owned_daemon(CLEANUP_TIMEOUT, CLEANUP_POLL_INTERVAL)
        .await
        .expect("replacement-owned daemon cleanup should succeed");

    assert!(cleaned);
    assert_eq!(state.snapshot_pid(), None);
    assert!(process_is_not_running(pid));
}

fn process_is_not_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("run kill -0");
        !status.success()
    }

    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output()
            .expect("run tasklist");
        let stdout = String::from_utf8_lossy(&output.stdout);
        !stdout.contains(&pid.to_string())
    }
}
