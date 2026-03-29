//! # Task Registry
//!
//! Centralized async task lifecycle management using `CancellationToken` + `JoinSet`.
//!
//! All long-lived spawned tasks are tracked here, enabling graceful shutdown
//! with cooperative cancellation and bounded join timeout.

use std::time::Duration;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Centralized registry for tracking and managing long-lived async tasks.
///
/// Provides:
/// - `spawn()` to track tasks with a `CancellationToken` for cooperative shutdown
/// - `shutdown()` to cancel all tasks and join with a bounded timeout
/// - `child_token()` for creating subordinate cancellation tokens
/// - `token()` for direct access to the root cancellation token
pub struct TaskRegistry {
    token: CancellationToken,
    tasks: tokio::sync::Mutex<JoinSet<()>>,
}

impl TaskRegistry {
    /// Create a new empty TaskRegistry.
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            tasks: tokio::sync::Mutex::new(JoinSet::new()),
        }
    }

    /// Get a child token that is cancelled when the root token is cancelled.
    pub fn child_token(&self) -> CancellationToken {
        self.token.child_token()
    }

    /// Get a reference to the root cancellation token.
    ///
    /// Used by the app exit hook to signal shutdown without calling the full
    /// `shutdown()` method (which requires async context).
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// Spawn a tracked task that receives a `CancellationToken` for cooperative cancellation.
    ///
    /// The task is added to the internal `JoinSet` and will be joined during `shutdown()`.
    /// A child token is created for each task so cancelling the root token cascades.
    pub async fn spawn<F, Fut>(&self, name: &'static str, f: F)
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let token = self.token.child_token();
        let mut tasks = self.tasks.lock().await;
        tasks.spawn(async move {
            f(token).await;
            debug!(task = name, "Task completed");
        });
    }

    /// Returns the number of currently tracked tasks.
    pub async fn task_count(&self) -> usize {
        self.tasks.lock().await.len()
    }

    /// Cancel all tracked tasks and join with a bounded timeout.
    ///
    /// 1. Cancels the root token (propagates to all child tokens)
    /// 2. Awaits `join_next()` in a loop with a deadline
    /// 3. If the deadline fires before all tasks join, aborts the remaining tasks
    pub async fn shutdown(&self, timeout_duration: Duration) {
        info!("Initiating graceful shutdown");
        self.token.cancel();

        let mut tasks = self.tasks.lock().await;
        let deadline = tokio::time::sleep(timeout_duration);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                result = tasks.join_next() => {
                    match result {
                        Some(Ok(())) => {}
                        Some(Err(e)) => warn!(error = %e, "Task join error"),
                        None => {
                            info!("All tasks joined cleanly");
                            return;
                        }
                    }
                }
                _ = &mut deadline => {
                    warn!(remaining = tasks.len(), "Shutdown timeout reached, aborting remaining tasks");
                    tasks.abort_all();
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn shutdown_cancels_all_tasks_and_joins_cleanly() {
        let registry = TaskRegistry::new();
        let counter = Arc::new(AtomicU32::new(0));

        for _ in 0..3 {
            let c = counter.clone();
            registry
                .spawn("test_task", |token| async move {
                    token.cancelled().await;
                    c.fetch_add(1, Ordering::SeqCst);
                })
                .await;
        }

        registry.shutdown(Duration::from_secs(5)).await;
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn shutdown_timeout_aborts_stuck_tasks() {
        let registry = TaskRegistry::new();
        let completed = Arc::new(AtomicU32::new(0));

        // Spawn a task that ignores cancellation (sleeps forever)
        registry
            .spawn("stuck_task", |_token| async move {
                // Intentionally ignoring the token - simulates a non-cooperative task
                tokio::time::sleep(Duration::from_secs(3600)).await;
            })
            .await;

        let c = completed.clone();
        registry
            .spawn("good_task", |token| async move {
                token.cancelled().await;
                c.fetch_add(1, Ordering::SeqCst);
            })
            .await;

        registry.shutdown(Duration::from_millis(200)).await;

        // The good task should have completed; the stuck task was aborted
        assert_eq!(completed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn child_token_cancelled_when_parent_cancelled() {
        let registry = TaskRegistry::new();
        let child = registry.child_token();

        assert!(!child.is_cancelled());
        registry.token().cancel();
        // child_token should now be cancelled
        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn spawn_tracks_task_count() {
        let registry = TaskRegistry::new();

        assert_eq!(registry.task_count().await, 0);

        registry
            .spawn("task_a", |token| async move {
                token.cancelled().await;
            })
            .await;

        registry
            .spawn("task_b", |token| async move {
                token.cancelled().await;
            })
            .await;

        assert_eq!(registry.task_count().await, 2);

        // Clean up
        registry.shutdown(Duration::from_secs(1)).await;
    }
}
