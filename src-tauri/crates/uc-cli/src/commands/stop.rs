//! Stop command -- terminates the running daemon gracefully via SIGTERM.

use std::fmt;
use std::time::Duration;

use serde::Serialize;

use crate::exit_codes;
use crate::output;

const STOP_TIMEOUT: Duration = Duration::from_secs(10);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Serialize)]
pub struct StopOutput {
    pub status: &'static str,
}

impl fmt::Display for StopOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            "stopped" => write!(f, "Daemon stopped"),
            "not_running" => write!(f, "Daemon is not running"),
            status => write!(f, "Daemon {}", status),
        }
    }
}

/// Run the stop command.
pub async fn run(json: bool, _verbose: bool) -> i32 {
    run_stop_with(
        || uc_daemon_local::process_metadata::read_pid_file(),
        |pid| is_process_running(pid),
        |pid| send_sigterm(pid),
        json,
    )
    .await
}

/// Testable inner implementation that accepts injectable closures.
///
/// `read_pid` returns `Result<Option<u32>>` — the daemon PID if running.
/// `is_process_running` checks whether a process with the given PID exists.
/// `send_sigterm` sends SIGTERM to the given PID; returns `true` on success.
pub(crate) async fn run_stop_with<ReadPid, IsRunning, SendSignal>(
    read_pid: ReadPid,
    is_process_running: IsRunning,
    send_sigterm: SendSignal,
    json: bool,
) -> i32
where
    ReadPid: FnOnce() -> anyhow::Result<Option<u32>>,
    IsRunning: Fn(u32) -> bool,
    SendSignal: FnOnce(u32) -> bool,
{
    // Step 1: Read PID file.
    let pid = match read_pid() {
        Ok(None) => {
            let out = StopOutput {
                status: "not_running",
            };
            if let Err(e) = output::print_result(&out, json) {
                eprintln!("Error: {}", e);
                return exit_codes::EXIT_ERROR;
            }
            return exit_codes::EXIT_SUCCESS;
        }
        Ok(Some(pid)) => pid,
        Err(e) => {
            eprintln!("Error: failed to read daemon PID file: {}", e);
            return exit_codes::EXIT_ERROR;
        }
    };

    // Step 2: Check if process is actually running (stale PID file guard).
    if !is_process_running(pid) {
        let out = StopOutput {
            status: "not_running",
        };
        if let Err(e) = output::print_result(&out, json) {
            eprintln!("Error: {}", e);
            return exit_codes::EXIT_ERROR;
        }
        return exit_codes::EXIT_SUCCESS;
    }

    // Step 3: Send SIGTERM.
    if !send_sigterm(pid) {
        eprintln!("Error: failed to send stop signal to daemon (pid {})", pid);
        return exit_codes::EXIT_ERROR;
    }

    // Step 4: Poll until process exits or timeout.
    let deadline = std::time::Instant::now() + STOP_TIMEOUT;
    loop {
        tokio::time::sleep(STOP_POLL_INTERVAL).await;

        if !is_process_running(pid) {
            break;
        }

        if std::time::Instant::now() >= deadline {
            eprintln!(
                "Warning: daemon (pid {}) did not stop within {}s. You may need to terminate it manually.",
                pid,
                STOP_TIMEOUT.as_secs()
            );
            return exit_codes::EXIT_ERROR;
        }
    }

    // Step 5: Report success.
    let out = StopOutput { status: "stopped" };
    if let Err(e) = output::print_result(&out, json) {
        eprintln!("Error: {}", e);
        return exit_codes::EXIT_ERROR;
    }

    exit_codes::EXIT_SUCCESS
}

#[cfg(unix)]
pub(crate) fn is_process_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(unix)]
pub(crate) fn send_sigterm(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) == 0 }
}

#[cfg(windows)]
pub(crate) fn is_process_running(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(windows)]
pub(crate) fn send_sigterm(pid: u32) -> bool {
    std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
