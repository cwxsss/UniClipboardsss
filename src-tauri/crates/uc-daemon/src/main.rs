//! UniClipboard daemon binary entry point.
//!
//! Bootstraps via `build_daemon_app()` for config/paths, creates services,
//! and runs `DaemonApp` in a tokio runtime.
//!
//! This is the composition root: typed services are built here, then erased
//! to `Arc<dyn DaemonService>` for uniform lifecycle management by DaemonApp.

use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use uc_app::usecases::LoggingLifecycleEventEmitter;
use uc_bootstrap::assembly::SetupAssemblyPorts;
use uc_bootstrap::build_non_gui_runtime_with_setup;
use uc_bootstrap::builders::build_daemon_app;
use uc_daemon::api::types::DaemonWsEvent;
use uc_daemon::app::DaemonApp;
use uc_daemon::pairing::host::DaemonPairingHost;
use uc_daemon::peers::monitor::PeerMonitor;
use uc_daemon::service::DaemonService;
use uc_daemon::socket::resolve_daemon_socket_path;
use uc_daemon::state::{DaemonServiceSnapshot, RuntimeState};
use uc_daemon::service::ServiceHealth;
use uc_daemon::workers::clipboard_watcher::ClipboardWatcherWorker;
use uc_daemon::workers::peer_discovery::PeerDiscoveryWorker;

fn main() -> anyhow::Result<()> {
    // build_daemon_app() calls build_core() which inits tracing + wires deps.
    // Safe to call outside tokio (no internal block_on in daemon path).
    let ctx = build_daemon_app()?;
    let daemon_network_control = ctx.deps.network_control.clone();
    let daemon_network_events = ctx.deps.network_ports.events.clone();
    let daemon_peer_directory = ctx.deps.network_ports.peers.clone();
    let daemon_settings = ctx.deps.settings.clone();
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

    // 1. Create the shared broadcast channel for WebSocket events.
    //    All services that emit WS events write to this same sender.
    let (event_tx, _) = broadcast::channel::<DaemonWsEvent>(64);

    // 2. Create shared runtime state (used by DaemonPairingHost for session snapshots
    //    and by DaemonApp for service health reporting).
    let initial_statuses: Vec<DaemonServiceSnapshot> = vec![
        DaemonServiceSnapshot {
            name: "clipboard-watcher".to_string(),
            health: ServiceHealth::Healthy,
        },
        DaemonServiceSnapshot {
            name: "peer-discovery".to_string(),
            health: ServiceHealth::Healthy,
        },
        DaemonServiceSnapshot {
            name: "pairing-host".to_string(),
            health: ServiceHealth::Healthy,
        },
        DaemonServiceSnapshot {
            name: "peer-monitor".to_string(),
            health: ServiceHealth::Healthy,
        },
    ];
    let state = Arc::new(RwLock::new(RuntimeState::new(initial_statuses)));

    // 3. Build typed PairingHost (per D-07: construction before DaemonApp).
    //    Typed Arc is kept for DaemonApiState so HTTP routes retain typed access (PH56-04).
    let pairing_host = Arc::new(DaemonPairingHost::new(
        runtime.clone(),
        ctx.pairing_orchestrator.clone(),
        ctx.pairing_action_rx,
        state.clone(),
        ctx.space_access_orchestrator.clone(),
        ctx.key_slot_store.clone(),
        event_tx.clone(),
    ));

    // 4. Build PeerMonitor (extracted from PairingHost in Plan 02).
    let peer_monitor = Arc::new(PeerMonitor::new(runtime.clone(), event_tx.clone()));

    // 5. Assemble services vec — typed services erased to Arc<dyn DaemonService> (per D-05).
    //    Order matters for shutdown (reversed): peer-monitor and pairing-host stop last.
    let services: Vec<Arc<dyn DaemonService>> = vec![
        Arc::new(ClipboardWatcherWorker) as Arc<dyn DaemonService>,
        Arc::new(PeerDiscoveryWorker::new(
            daemon_network_control,
            daemon_network_events,
            daemon_peer_directory,
            daemon_settings,
        )) as Arc<dyn DaemonService>,
        Arc::clone(&pairing_host) as Arc<dyn DaemonService>,
        Arc::clone(&peer_monitor) as Arc<dyn DaemonService>,
    ];

    // 6. Create and run daemon app.
    //    api_pairing_host retains typed access for HTTP routes.
    //    space_access_orchestrator is passed for API state wiring.
    let daemon = DaemonApp::new(
        services,
        runtime,
        state,
        event_tx,
        Some(pairing_host),
        Some(ctx.space_access_orchestrator),
        socket_path,
    );

    // Use explicit runtime construction (consistent with uc-bootstrap pattern,
    // avoids potential conflicts with tracing init's internal runtime for Seq)
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(daemon.run())
}
