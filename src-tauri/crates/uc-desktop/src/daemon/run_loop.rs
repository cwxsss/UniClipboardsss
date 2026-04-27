//! daemon 运行循环。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tokio::runtime::Runtime;
use tokio::sync::Notify;
use uc_application::facade::AppFacade;
use uc_bootstrap::SpaceSetupAssembly;
use uc_core::ports::SettingsPort;

use crate::daemon::app::DaemonApp;
use crate::daemon::run_mode::DaemonRunMode;
use crate::daemon::startup_recovery::{spawn_startup_recovery, StartupRecoveryInput};

/// daemon 运行循环输入。
pub struct DaemonRunLoopInput {
    pub run_mode: DaemonRunMode,
    pub daemon: DaemonApp,
    pub app_facade: Arc<AppFacade>,
    pub settings: Arc<dyn SettingsPort>,
    pub space_setup_assembly: SpaceSetupAssembly,
    pub deferred_ready_notify: Arc<Notify>,
    pub clipboard_capture_gate: Arc<AtomicBool>,
}

/// 运行 daemon，退出后关闭 space setup 资源。
pub fn run_daemon_until_shutdown(
    runtime: &Runtime,
    input: DaemonRunLoopInput,
) -> anyhow::Result<()> {
    let DaemonRunLoopInput {
        run_mode,
        daemon,
        app_facade,
        settings,
        space_setup_assembly,
        deferred_ready_notify,
        clipboard_capture_gate,
    } = input;

    let space_setup = space_setup_assembly.facade.clone();

    runtime.block_on(async move {
        spawn_startup_recovery(StartupRecoveryInput {
            run_mode,
            app_facade,
            settings,
            space_setup,
            deferred_ready_notify,
            clipboard_capture_gate,
        });

        let result = daemon.run().await;
        space_setup_assembly.shutdown().await;
        result
    })
}
