//! Startup orchestration commands
//! 启动流程编排命令

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tracing::info;

use crate::tray::show_main_window;

/// Startup barrier used to coordinate backend readiness.
///
/// 用于协调后端就绪的启动门闩。
///
/// # Behavior / 行为
/// - When backend is ready, it shows the main window.
/// - 当后端就绪时，显示主窗口。
#[derive(Default)]
pub struct StartupBarrier {
    backend_ready: AtomicBool,
    finished: AtomicBool,
}

impl StartupBarrier {
    /// Mark the backend as ready.
    ///
    /// 标记后端已就绪。
    pub fn mark_backend_ready(&self) {
        self.backend_ready.store(true, Ordering::SeqCst);
    }

    /// Try to finish startup once (idempotent).
    ///
    /// 尝试完成启动收尾（幂等）。
    pub fn try_finish(&self, app_handle: &AppHandle) {
        if self.finished.load(Ordering::SeqCst) {
            return;
        }

        let backend_ready = self.backend_ready.load(Ordering::SeqCst);
        if !backend_ready {
            info!(backend_ready, "StartupBarrier not ready to finish yet");
            return;
        }

        if self
            .finished
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }

        show_main_window(app_handle);
        info!("Main window show requested (startup barrier)");
    }
}
