//! daemon 后台任务启动。

use std::sync::Arc;

use tokio::runtime::Runtime;
use uc_bootstrap::{BackgroundRuntimeDeps, BlobProcessingPorts, TaskRegistry};

/// 启动 daemon 需要的后台 blob 处理任务。
pub fn spawn_daemon_background_tasks(
    runtime: &Runtime,
    background: BackgroundRuntimeDeps,
    blob_ports: BlobProcessingPorts,
    task_registry: Arc<TaskRegistry>,
) {
    runtime.spawn(async move {
        uc_bootstrap::spawn_blob_processing_tasks(background, blob_ports, &task_registry).await;
    });
}
