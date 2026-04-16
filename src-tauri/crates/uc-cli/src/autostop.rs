//! Autostop daemon guard -- RAII type that stops a CLI-spawned daemon on drop.
//!
//! Used by one-shot commands (`setup reset`, `setup connect`, etc.) that auto-start
//! the daemon via `ensure_local_daemon_running`. When the command function returns,
//! the guard's `Drop` impl sends SIGTERM to the daemon PID — but only if the guard
//! was armed (meaning the command itself spawned the daemon; pre-existing daemons
//! are left alone).
//!
//! The `#[autostop]` attribute macro in `uc-cli-macros` is the ergonomic way to
//! apply this guard to command functions.

use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::commands::stop::{is_process_running, send_sigterm};
use crate::local_daemon::LocalDaemonSession;

/// Graceful shutdown timeout in the autostop path.
///
/// Kept short (3s) so CLI exits feel snappy. If the daemon is still alive after
/// this window we just print a warning and leak it — we never escalate to SIGKILL.
pub(crate) const AUTOSTOP_TIMEOUT: Duration = Duration::from_secs(3);
pub(crate) const AUTOSTOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// RAII guard that stops the daemon on drop if it was spawned by this CLI run.
///
/// Construct via [`AutostopGuard::arm`]. The guard is "armed" only when
/// `session.spawned == true`; otherwise it's a noop.
///
/// SIGTERM + `waitpid`-like polling are synchronous, so this works safely inside
/// `Drop` even when the surrounding function is async.
#[must_use = "AutostopGuard must be bound to a local variable, or it drops immediately and kills the daemon"]
pub struct AutostopGuard {
    /// `Some(pid)` = armed; `None` = disarmed / noop.
    pid: Option<u32>,
}

impl AutostopGuard {
    /// Arm the guard iff the daemon was just spawned by this CLI run.
    ///
    /// PID is resolved lazily on drop via the daemon PID file, so callers don't
    /// need to pass it in. The session is consulted only for the `spawned` flag.
    pub fn arm(session: &LocalDaemonSession) -> Self {
        if !session.spawned {
            return Self { pid: None };
        }
        // PID resolved on drop (file may not be flushed at arm time in edge cases).
        // We use a sentinel 0 to mean "arm-and-resolve-later"; real PIDs are never 0.
        Self { pid: Some(0) }
    }

    /// Construct an unarmed (noop) guard.
    #[allow(dead_code)]
    pub fn noop() -> Self {
        Self { pid: None }
    }

    /// Disarm the guard so Drop becomes a noop.
    ///
    /// Useful when a command decides mid-flow that it wants to keep the daemon
    /// alive (for example, `uc start` deliberately leaves it running).
    #[allow(dead_code)]
    pub fn disarm(&mut self) {
        self.pid = None;
    }
}

impl Drop for AutostopGuard {
    fn drop(&mut self) {
        let armed = self.pid.take();
        if armed.is_none() {
            return;
        }

        let pid = match uc_daemon::process_metadata::read_pid_file() {
            Ok(Some(pid)) => pid,
            Ok(None) => return, // daemon already gone or never wrote PID file
            Err(e) => {
                eprintln!("Warning: autostop could not read daemon PID file: {e}");
                return;
            }
        };

        if !is_process_running(pid) {
            return;
        }

        if !send_sigterm(pid) {
            eprintln!(
                "Warning: autostop could not send SIGTERM to daemon (pid {pid}); daemon left running"
            );
            return;
        }

        let deadline = Instant::now() + AUTOSTOP_TIMEOUT;
        loop {
            sleep(AUTOSTOP_POLL_INTERVAL);
            if !is_process_running(pid) {
                return;
            }
            if Instant::now() >= deadline {
                eprintln!(
                    "Warning: daemon (pid {pid}) did not stop within {}s; left running",
                    AUTOSTOP_TIMEOUT.as_secs()
                );
                return;
            }
        }
    }
}
