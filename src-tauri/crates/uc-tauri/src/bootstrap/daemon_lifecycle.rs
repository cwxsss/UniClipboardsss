use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

    pub fn finish_exit_cleanup(&self) {
        self.0.exit_in_progress.store(false, Ordering::SeqCst);
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
