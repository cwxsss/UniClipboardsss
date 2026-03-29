//! Integration tests for TaskRegistry.
//!
//! These tests verify the core lifecycle management behaviors:
//! - Spawn/shutdown with cooperative cancellation
//! - Timeout abort for non-cooperative tasks
//! - Child token propagation
//! - Task count tracking

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uc_app::task_registry::TaskRegistry;

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
