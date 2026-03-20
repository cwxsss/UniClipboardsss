//! Bootstrap module - Application initialization and wiring
//! Bootstrap 模块 - 应用初始化和连接

pub mod daemon_ws_bridge {
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::Result;
    use tokio::sync::mpsc;

    use super::DaemonConnectionState;
    use uc_core::ports::{RealtimeEvent, RealtimeTopic};
    use uc_daemon::api::types::DaemonWsEvent;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BridgeState {
        Disconnected,
        Connecting,
        Subscribing,
        Ready,
        Degraded,
    }

    #[derive(Debug, Clone)]
    pub struct DaemonWsBridgeConfig {
        pub queue_capacity: usize,
        pub terminal_retry_delay: Duration,
        pub backoff_initial: Duration,
        pub backoff_max: Duration,
    }

    impl Default for DaemonWsBridgeConfig {
        fn default() -> Self {
            Self {
                queue_capacity: 64,
                terminal_retry_delay: Duration::from_millis(50),
                backoff_initial: Duration::from_millis(250),
                backoff_max: Duration::from_millis(30_000),
            }
        }
    }

    #[derive(Debug, Default)]
    pub struct ScriptedDaemonWsConnector;

    impl ScriptedDaemonWsConnector {
        pub fn new() -> Arc<Self> {
            todo!("implemented in plan 46.1-03 task 2")
        }

        pub async fn queue_connection(&self, _events: Vec<DaemonWsEvent>) -> Result<()> {
            todo!("implemented in plan 46.1-03 task 2")
        }

        pub fn connect_attempts(&self) -> usize {
            todo!("implemented in plan 46.1-03 task 2")
        }

        pub fn subscribe_requests(&self) -> Vec<Vec<String>> {
            todo!("implemented in plan 46.1-03 task 2")
        }

        pub fn auth_headers(&self) -> Vec<String> {
            todo!("implemented in plan 46.1-03 task 2")
        }
    }

    #[derive(Debug)]
    pub struct DaemonWsBridge;

    impl DaemonWsBridge {
        pub fn new_for_test(
            _connection_state: DaemonConnectionState,
            _connector: Arc<ScriptedDaemonWsConnector>,
            _config: DaemonWsBridgeConfig,
        ) -> Self {
            todo!("implemented in plan 46.1-03 task 2")
        }

        pub async fn subscribe(
            &self,
            _consumer: &'static str,
            _topics: &[RealtimeTopic],
        ) -> Result<mpsc::Receiver<RealtimeEvent>> {
            todo!("implemented in plan 46.1-03 task 2")
        }

        pub async fn run_until_idle(&self) -> Result<()> {
            todo!("implemented in plan 46.1-03 task 2")
        }

        pub fn state(&self) -> BridgeState {
            todo!("implemented in plan 46.1-03 task 2")
        }
    }
}

pub mod assembly;
pub mod clipboard_integration_mode;
pub mod config;
pub mod config_resolution;
pub mod file_transfer_wiring;
pub mod init;
pub mod logging;
pub mod pairing_bridge;
pub mod run;
pub mod runtime;
pub mod setup_pairing_bridge;
pub mod task_registry;
pub mod tracing;
pub mod wiring;

// Re-export commonly used bootstrap functions
pub use assembly::SetupAssemblyPorts;
pub use clipboard_integration_mode::resolve_clipboard_integration_mode;
pub use config::load_config;
pub use config_resolution::{resolve_app_config, resolve_config_path, ConfigResolutionError};
pub use daemon_ws_bridge::DaemonWsBridge;
pub use init::ensure_default_device_name;
pub use pairing_bridge::PairingBridge;
pub use run::{bootstrap_daemon_connection, emit_daemon_connection_info_if_ready};
pub use runtime::{create_app, create_runtime, AppRuntime, AppUseCases, DaemonConnectionState};
pub use setup_pairing_bridge::{build_setup_pairing_facade, DaemonBackedSetupPairingFacade};
pub use uc_app::usecases::setup::SetupPairingFacadePort;
// assembly.rs re-exports (pure dependency construction — zero tauri imports)
pub use assembly::{
    build_setup_orchestrator, get_storage_paths, resolve_pairing_config,
    resolve_pairing_device_name, wire_dependencies, WiredDependencies,
};
// wiring.rs re-exports (Tauri event loops and background task management)
pub use wiring::{start_background_tasks, BackgroundRuntimeDeps};
