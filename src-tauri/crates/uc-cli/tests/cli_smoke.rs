//! CLI smoke tests — validates binary invocation, help output, exit codes, and version flag.

use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn cli_binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_uniclipboard-cli"))
}

fn smoke_test_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn test_help_output() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .arg("--help")
        .output()
        .expect("failed to execute uniclipboard-cli");

    assert!(
        output.status.success(),
        "Expected exit code 0 for --help, got {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("status"),
        "Help output should mention 'status' subcommand, got: {}",
        stdout
    );
    assert!(
        stdout.contains("devices"),
        "Help output should mention 'devices' subcommand, got: {}",
        stdout
    );
    assert!(
        stdout.contains("space-status"),
        "Help output should mention 'space-status' subcommand, got: {}",
        stdout
    );
    assert!(
        stdout.contains("clipboard"),
        "Help output should mention 'clipboard' subcommand, got: {}",
        stdout
    );
}

#[test]
fn test_version_flag() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .arg("--version")
        .output()
        .expect("failed to execute uniclipboard-cli");

    assert!(
        output.status.success(),
        "Expected exit code 0 for --version, got {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("uniclipboard-cli"),
        "Version output should contain binary name, got: {}",
        stdout
    );
}

#[test]
fn test_status_daemon_unreachable() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .arg("status")
        .output()
        .expect("failed to execute uniclipboard-cli");

    let exit_code = output.status.code().expect("process terminated by signal");
    assert_eq!(
        exit_code, 5,
        "Expected exit code 5 (daemon unreachable), got {}",
        exit_code
    );
}

#[test]
fn test_status_json_daemon_unreachable() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .args(["--json", "status"])
        .output()
        .expect("failed to execute uniclipboard-cli");

    let exit_code = output.status.code().expect("process terminated by signal");
    assert_eq!(
        exit_code, 5,
        "Expected exit code 5 (daemon unreachable) with --json, got {}",
        exit_code
    );
}

// ---------------------------------------------------------------------------
// Clipboard CLI tests
// ---------------------------------------------------------------------------

// TODO: Tests with actual clipboard data require a seeded test database.
// Approach: use uc-bootstrap to create a temp storage, insert entries via
// CoreUseCases, then invoke the CLI binary against that storage.
// For now, tests cover empty-state behavior and error paths.

#[test]
fn test_clipboard_list_empty_history() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .args(["clipboard", "list"])
        .output()
        .expect("failed to execute uniclipboard-cli");

    assert!(
        output.status.success(),
        "Expected exit code 0 for clipboard list, got {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No clipboard entries found."),
        "Empty list should show 'No clipboard entries found.', got: {}",
        stdout
    );
}

#[test]
fn test_clipboard_list_json_empty_history() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .args(["clipboard", "list", "--json"])
        .output()
        .expect("failed to execute uniclipboard-cli");

    assert!(
        output.status.success(),
        "Expected exit code 0 for clipboard list --json, got {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    assert_eq!(parsed["count"], 0, "count should be 0");
    assert!(parsed["entries"].is_array(), "entries should be an array");
    assert_eq!(
        parsed["entries"].as_array().unwrap().len(),
        0,
        "entries array should be empty"
    );
}

#[test]
fn test_clipboard_get_nonexistent_entry() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .args(["clipboard", "get", "non-existent-id"])
        .output()
        .expect("failed to execute uniclipboard-cli");

    let exit_code = output.status.code().expect("process terminated by signal");
    assert_eq!(
        exit_code, 1,
        "Expected exit code 1 for get with non-existent ID, got {}",
        exit_code
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Error"),
        "stderr should contain error message, got: {}",
        stderr
    );
}

#[test]
fn test_clipboard_clear_empty_history() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .args(["clipboard", "clear"])
        .output()
        .expect("failed to execute uniclipboard-cli");

    assert!(
        output.status.success(),
        "Expected exit code 0 for clipboard clear, got {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Cleared 0 clipboard entries."),
        "Empty clear should show 'Cleared 0 clipboard entries.', got: {}",
        stdout
    );
}

#[test]
fn test_clipboard_clear_json_empty_history() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .args(["clipboard", "clear", "--json"])
        .output()
        .expect("failed to execute uniclipboard-cli");

    assert!(
        output.status.success(),
        "Expected exit code 0 for clipboard clear --json, got {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("invalid JSON output");
    assert_eq!(parsed["deleted_count"], 0, "deleted_count should be 0");
    assert_eq!(parsed["failed_count"], 0, "failed_count should be 0");
}

#[test]
fn test_clipboard_list_with_limit_and_offset() {
    let _guard = smoke_test_guard();
    let output = cli_binary()
        .args(["clipboard", "list", "--limit", "10", "--offset", "0"])
        .output()
        .expect("failed to execute uniclipboard-cli");

    assert!(
        output.status.success(),
        "Expected exit code 0 for clipboard list with limit/offset, got {:?}",
        output.status.code()
    );
}
