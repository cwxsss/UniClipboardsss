//! Bootstrap module - Application initialization and wiring
//! Bootstrap 模块 - 应用初始化和连接

pub mod logging;
pub mod run;
pub mod runtime;
pub mod wiring;

// Re-export commonly used bootstrap functions
pub use run::{
    bootstrap_daemon_connection, emit_daemon_connection_info_if_ready, supervise_daemon,
};
pub use runtime::{create_app, create_runtime, AppRuntime, AppUseCases};
pub use uc_bootstrap::assembly::SetupAssemblyPorts;
pub use uc_bootstrap::ensure_default_device_name;
pub use uc_bootstrap::load_config;
// uc_bootstrap re-exports (pure dependency construction — zero tauri imports)
pub use uc_bootstrap::assembly::{
    build_setup_orchestrator, get_storage_paths, resolve_pairing_config,
    resolve_pairing_device_name, wire_dependencies, wire_dependencies_with_identity_store,
    WiredDependencies,
};
// wiring.rs re-exports (Tauri event loops and background task management)
pub use wiring::{start_background_tasks, BackgroundRuntimeDeps};
