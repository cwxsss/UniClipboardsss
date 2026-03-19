//! UniClipboard daemon binary entry point.
//!
//! Bootstraps via `build_daemon_app()` for config/paths, creates workers,
//! and runs `DaemonApp` in a tokio runtime.

use std::sync::Arc;

use uc_app::usecases::LoggingLifecycleEventEmitter;
use uc_bootstrap::assembly::SetupAssemblyPorts;
use uc_bootstrap::build_non_gui_runtime_with_setup;
use uc_bootstrap::builders::build_daemon_app;
use uc_daemon::app::DaemonApp;
use uc_daemon::socket::resolve_daemon_socket_path;
use uc_daemon::worker::DaemonWorker;
use uc_daemon::workers::clipboard_watcher::ClipboardWatcherWorker;
use uc_daemon::workers::peer_discovery::PeerDiscoveryWorker;

fn main() -> anyhow::Result<()> {
    // build_daemon_app() calls build_core() which inits tracing + wires deps.
    // Safe to call outside tokio (no internal block_on in daemon path).
    let ctx = build_daemon_app()?;
    let setup_ports = SetupAssemblyPorts::from_network(
        ctx.pairing_orchestrator.clone(),
        ctx.space_access_orchestrator.clone(),
        ctx.deps.network_ports.peers.clone(),
        None,
        Arc::new(LoggingLifecycleEventEmitter),
    );
    let runtime = Arc::new(build_non_gui_runtime_with_setup(
        ctx.deps,
        ctx.storage_paths.clone(),
        setup_ports,
        ctx.watcher_control.clone(),
    )?);

    let socket_path = resolve_daemon_socket_path();

    // Create workers (Arc-wrapped for tokio::spawn compatibility)
    let workers: Vec<Arc<dyn DaemonWorker>> = vec![
        Arc::new(ClipboardWatcherWorker),
        Arc::new(PeerDiscoveryWorker),
    ];

    // Create and run daemon app
    let daemon = DaemonApp::new(
        workers,
        runtime,
        ctx.pairing_orchestrator,
        ctx.pairing_action_rx,
        ctx.staged_store,
        ctx.space_access_orchestrator,
        ctx.key_slot_store,
        socket_path,
    );

    // Use explicit runtime construction (consistent with uc-bootstrap pattern,
    // avoids potential conflicts with tracing init's internal runtime for Seq)
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(daemon.run())
}
