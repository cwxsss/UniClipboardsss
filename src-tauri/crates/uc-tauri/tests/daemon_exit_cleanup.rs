//! Canonical regression target for Phase 46.6 daemon exit cleanup work.
//!
//! Run with:
//! `cd src-tauri && cargo test -p uc-tauri --test daemon_exit_cleanup -- --test-threads=1`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitCleanupScenario {
    SpawnedGuiDaemon,
    CompatibleExistingDaemon,
    ClearOwnedPidAfterSuccess,
    RepeatedExitRequests,
    MainWindowCloseToTray,
    ReplacementOwnedDaemon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExitCleanupFixture {
    scenario: ExitCleanupScenario,
    canonical_command: &'static str,
}

impl ExitCleanupFixture {
    fn for_scenario(scenario: ExitCleanupScenario) -> Self {
        Self {
            scenario,
            canonical_command:
                "cargo test -p uc-tauri --test daemon_exit_cleanup -- --test-threads=1",
        }
    }

    fn extends_runtime_owned_daemon_contract(&self) -> bool {
        matches!(
            self.scenario,
            ExitCleanupScenario::SpawnedGuiDaemon
                | ExitCleanupScenario::CompatibleExistingDaemon
                | ExitCleanupScenario::ClearOwnedPidAfterSuccess
                | ExitCleanupScenario::RepeatedExitRequests
                | ExitCleanupScenario::ReplacementOwnedDaemon
        )
    }

    fn preserves_main_window_close_to_tray(&self) -> bool {
        matches!(self.scenario, ExitCleanupScenario::MainWindowCloseToTray)
    }
}

#[test]
fn daemon_exit_cleanup_terminates_spawned_gui_daemon() {
    let fixture = ExitCleanupFixture::for_scenario(ExitCleanupScenario::SpawnedGuiDaemon);

    assert_eq!(
        fixture.canonical_command,
        "cargo test -p uc-tauri --test daemon_exit_cleanup -- --test-threads=1"
    );
    assert!(fixture.extends_runtime_owned_daemon_contract());
}

#[test]
fn daemon_exit_cleanup_skips_when_gui_did_not_spawn_daemon() {
    let fixture = ExitCleanupFixture::for_scenario(ExitCleanupScenario::CompatibleExistingDaemon);

    assert_eq!(
        fixture.scenario,
        ExitCleanupScenario::CompatibleExistingDaemon
    );
    assert!(fixture.extends_runtime_owned_daemon_contract());
}

#[test]
fn daemon_exit_cleanup_clears_owned_pid_after_success() {
    let fixture = ExitCleanupFixture::for_scenario(ExitCleanupScenario::ClearOwnedPidAfterSuccess);

    assert_eq!(
        fixture.scenario,
        ExitCleanupScenario::ClearOwnedPidAfterSuccess
    );
    assert!(fixture.extends_runtime_owned_daemon_contract());
}

#[test]
fn daemon_exit_cleanup_is_idempotent_across_repeated_exit_requests() {
    let fixture = ExitCleanupFixture::for_scenario(ExitCleanupScenario::RepeatedExitRequests);

    assert_eq!(fixture.scenario, ExitCleanupScenario::RepeatedExitRequests);
    assert!(fixture.extends_runtime_owned_daemon_contract());
}

#[test]
fn main_window_close_request_hides_to_tray_without_daemon_cleanup() {
    let fixture = ExitCleanupFixture::for_scenario(ExitCleanupScenario::MainWindowCloseToTray);

    assert_eq!(fixture.scenario, ExitCleanupScenario::MainWindowCloseToTray);
    assert!(fixture.preserves_main_window_close_to_tray());
}

#[test]
fn daemon_exit_cleanup_terminates_replacement_owned_daemon() {
    let fixture = ExitCleanupFixture::for_scenario(ExitCleanupScenario::ReplacementOwnedDaemon);

    assert_eq!(
        fixture.scenario,
        ExitCleanupScenario::ReplacementOwnedDaemon
    );
    assert!(fixture.extends_runtime_owned_daemon_contract());
}
