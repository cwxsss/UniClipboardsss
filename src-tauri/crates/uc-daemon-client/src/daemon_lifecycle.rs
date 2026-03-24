//! GUI-owned daemon process lifecycle management.
//! Handles spawned daemon child tracking, graceful shutdown, and exit cleanup.

use std::process::Child;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use tokio::time::{sleep, Instant};

#[derive(Debug)]
pub struct TerminateDaemonError(pub String);

impl std::fmt::Display for TerminateDaemonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for TerminateDaemonError {}

/// Terminates a local daemon process by PID using platform-specific commands.
/// Returns `TerminateDaemonError` on failure.
pub fn terminate_local_daemon_pid(pid: u32) -> Result<(), TerminateDaemonError> {
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("kill");
        command.arg("-TERM").arg(pid.to_string());
        command
    };

    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("taskkill");
        command.arg("/PID").arg(pid.to_string()).arg("/T").arg("/F");
        command
    };

    let output = command
        .output()
        .map_err(|e| TerminateDaemonError(format!("failed to launch terminator: {e}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(TerminateDaemonError(format!(
        "failed to terminate pid {pid}: status={} stdout={} stderr={}",
        output.status,
        stdout.trim(),
        stderr.trim()
    )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnReason {
    Absent,
    Replacement,
}

#[derive(Debug)]
pub struct OwnedDaemonChild {
    pub pid: u32,
    pub spawn_reason: SpawnReason,
    pub child: Child,
}

#[derive(Default)]
struct GuiOwnedDaemonStateInner {
    child: Mutex<Option<OwnedDaemonChild>>,
    exit_in_progress: AtomicBool,
}

#[derive(Clone, Default)]
pub struct GuiOwnedDaemonState(Arc<GuiOwnedDaemonStateInner>);

#[derive(Debug, Error)]
pub enum DaemonExitCleanupError {
    #[error("failed to terminate GUI-owned daemon pid {pid}: {details}")]
    Terminate { pid: u32, details: String },
    #[error("failed to observe GUI-owned daemon pid {pid} exit: {source}")]
    Observe {
        pid: u32,
        #[source]
        source: std::io::Error,
    },
    #[error("timed out waiting {timeout_ms}ms for GUI-owned daemon pid {pid} to exit")]
    Timeout { pid: u32, timeout_ms: u64 },
    #[error("failed to force kill GUI-owned daemon pid {pid}: {source}")]
    ForceKill {
        pid: u32,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to reap GUI-owned daemon pid {pid}: {source}")]
    Wait {
        pid: u32,
        #[source]
        source: std::io::Error,
    },
}

impl GuiOwnedDaemonState {
    pub fn record_spawned(&self, child: Child, spawn_reason: SpawnReason) {
        let owned_child = OwnedDaemonChild {
            pid: child.id(),
            spawn_reason,
            child,
        };

        match self.0.child.lock() {
            Ok(mut guard) => {
                *guard = Some(owned_child);
            }
            Err(poisoned) => {
                tracing::error!(
                    "Mutex poisoned in GuiOwnedDaemonState::record_spawned, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                *guard = Some(owned_child);
            }
        }
    }

    pub fn clear(&self) -> Option<OwnedDaemonChild> {
        match self.0.child.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => {
                tracing::error!(
                    "Mutex poisoned in GuiOwnedDaemonState::clear, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                guard.take()
            }
        }
    }

    pub fn snapshot_pid(&self) -> Option<u32> {
        match self.0.child.lock() {
            Ok(guard) => guard.as_ref().map(|owned_child| owned_child.pid),
            Err(poisoned) => {
                tracing::error!(
                    "Mutex poisoned in GuiOwnedDaemonState::snapshot_pid, recovering from poisoned state"
                );
                let guard = poisoned.into_inner();
                guard.as_ref().map(|owned_child| owned_child.pid)
            }
        }
    }

    pub fn begin_exit_cleanup(&self) -> bool {
        !self.0.exit_in_progress.swap(true, Ordering::SeqCst)
    }

    pub fn exit_cleanup_in_progress(&self) -> bool {
        self.0.exit_in_progress.load(Ordering::SeqCst)
    }

    pub fn finish_exit_cleanup(&self) {
        self.0.exit_in_progress.store(false, Ordering::SeqCst);
    }

    pub async fn shutdown_owned_daemon(
        &self,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<bool, DaemonExitCleanupError> {
        let Some(mut owned_child) = self.take_owned_child() else {
            return Ok(false);
        };

        let daemon_pid = owned_child.pid;
        let spawn_reason = owned_child.spawn_reason;

        tracing::info!(
            daemon_pid,
            ?spawn_reason,
            timeout_ms = timeout.as_millis() as u64,
            poll_interval_ms = poll_interval.as_millis() as u64,
            "Starting GUI-owned daemon exit cleanup"
        );

        if let Err(error) = terminate_local_daemon_pid(daemon_pid) {
            match owned_child.child.try_wait() {
                Ok(Some(status)) => {
                    tracing::info!(
                        daemon_pid,
                        ?spawn_reason,
                        exit_status = %status,
                        "GUI-owned daemon already exited before cleanup wait"
                    );
                    return Ok(true);
                }
                Ok(None) => {
                    let cleanup_error = DaemonExitCleanupError::Terminate {
                        pid: daemon_pid,
                        details: error.to_string(),
                    };
                    self.restore_owned_child(owned_child);
                    return Err(cleanup_error);
                }
                Err(wait_error) => {
                    let cleanup_error = DaemonExitCleanupError::Observe {
                        pid: daemon_pid,
                        source: wait_error,
                    };
                    self.restore_owned_child(owned_child);
                    return Err(cleanup_error);
                }
            }
        }

        match wait_for_child_exit(&mut owned_child, timeout, poll_interval).await {
            Ok(()) => {
                tracing::info!(
                    daemon_pid,
                    ?spawn_reason,
                    "GUI-owned daemon exit cleanup completed"
                );
                Ok(true)
            }
            Err(DaemonExitCleanupError::Timeout { .. }) => {
                tracing::warn!(
                    daemon_pid,
                    ?spawn_reason,
                    timeout_ms = timeout.as_millis() as u64,
                    "GUI-owned daemon did not exit after graceful termination; forcing kill"
                );
                if let Err(error) = owned_child.child.kill() {
                    let cleanup_error = DaemonExitCleanupError::ForceKill {
                        pid: daemon_pid,
                        source: error,
                    };
                    self.restore_owned_child(owned_child);
                    return Err(cleanup_error);
                }

                if let Err(error) = owned_child.child.wait() {
                    let cleanup_error = DaemonExitCleanupError::Wait {
                        pid: daemon_pid,
                        source: error,
                    };
                    self.restore_owned_child(owned_child);
                    return Err(cleanup_error);
                }

                tracing::info!(
                    daemon_pid,
                    ?spawn_reason,
                    "GUI-owned daemon force kill completed"
                );
                Ok(true)
            }
            Err(error) => {
                self.restore_owned_child(owned_child);
                Err(error)
            }
        }
    }

    fn take_owned_child(&self) -> Option<OwnedDaemonChild> {
        match self.0.child.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => {
                tracing::error!(
                    "Mutex poisoned in GuiOwnedDaemonState::take_owned_child, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                guard.take()
            }
        }
    }

    fn restore_owned_child(&self, owned_child: OwnedDaemonChild) {
        match self.0.child.lock() {
            Ok(mut guard) => {
                *guard = Some(owned_child);
            }
            Err(poisoned) => {
                tracing::error!(
                    "Mutex poisoned in GuiOwnedDaemonState::restore_owned_child, recovering from poisoned state"
                );
                let mut guard = poisoned.into_inner();
                *guard = Some(owned_child);
            }
        }
    }
}

async fn wait_for_child_exit(
    owned_child: &mut OwnedDaemonChild,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), DaemonExitCleanupError> {
    let deadline = Instant::now() + timeout;

    loop {
        match owned_child.child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(error) => {
                return Err(DaemonExitCleanupError::Observe {
                    pid: owned_child.pid,
                    source: error,
                });
            }
        }

        if Instant::now() >= deadline {
            return Err(DaemonExitCleanupError::Timeout {
                pid: owned_child.pid,
                timeout_ms: timeout.as_millis() as u64,
            });
        }

        sleep(poll_interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn spawn_test_child() -> Child {
        Command::new(std::env::current_exe().expect("current test binary"))
            .arg("--help")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn test child")
    }

    fn cleanup_owned_child(state: &GuiOwnedDaemonState) {
        if let Some(mut owned_child) = state.clear() {
            let _ = owned_child.child.kill();
            let _ = owned_child.child.wait();
        }
    }

    #[test]
    fn record_spawned_tracks_pid_and_reason() {
        let state = GuiOwnedDaemonState::default();
        let child = spawn_test_child();
        let child_pid = child.id();

        state.record_spawned(child, SpawnReason::Absent);

        assert_eq!(state.snapshot_pid(), Some(child_pid));

        let owned_child = state.clear().expect("owned child should exist");
        assert_eq!(owned_child.pid, child_pid);
        assert_eq!(owned_child.spawn_reason, SpawnReason::Absent);

        let mut child = owned_child.child;
        let _ = child.kill();
        let _ = child.wait();
    }

    #[test]
    fn begin_exit_cleanup_is_idempotent_until_finished() {
        let state = GuiOwnedDaemonState::default();

        assert!(state.begin_exit_cleanup());
        assert!(!state.begin_exit_cleanup());

        state.finish_exit_cleanup();

        assert!(state.begin_exit_cleanup());
        state.finish_exit_cleanup();
    }

    #[test]
    fn clear_removes_owned_child_snapshot() {
        let state = GuiOwnedDaemonState::default();
        let child = spawn_test_child();

        state.record_spawned(child, SpawnReason::Replacement);
        cleanup_owned_child(&state);

        assert_eq!(state.snapshot_pid(), None);
    }
}
