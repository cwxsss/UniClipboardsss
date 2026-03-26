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
use uc_core::ports::SystemClipboardPort;
use uc_daemon::api::types::DaemonWsEvent;
use uc_daemon::app::DaemonApp;
use uc_daemon::pairing::host::DaemonPairingHost;
use uc_daemon::peers::monitor::PeerMonitor;
use uc_daemon::service::DaemonService;
use uc_daemon::service::ServiceHealth;
use uc_daemon::socket::resolve_daemon_socket_path;
use uc_daemon::state::{DaemonServiceSnapshot, RuntimeState};
use uc_daemon::workers::clipboard_watcher::{ClipboardWatcherWorker, DaemonClipboardChangeHandler};
use uc_daemon::workers::inbound_clipboard_sync::InboundClipboardSyncWorker;
use uc_daemon::workers::peer_discovery::PeerDiscoveryWorker;
use uc_infra::clipboard::InMemoryClipboardChangeOrigin;
use uc_platform::clipboard::LocalClipboard;

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
    // Extract file_cache_dir and file_transfer_orchestrator before ctx is consumed
    // by build_non_gui_runtime_with_setup (which moves ctx.deps).
    let file_cache_dir = ctx.storage_paths.file_cache_dir.clone();
    let file_transfer_orchestrator = ctx.background.file_transfer_orchestrator.clone();
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
            name: "inbound-clipboard-sync".to_string(),
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

    // 5a. Build LocalClipboard and ClipboardWatcherWorker (per D-02, D-07).
    let local_clipboard: Arc<dyn SystemClipboardPort> = Arc::new(
        LocalClipboard::new()
            .map_err(|e| anyhow::anyhow!("failed to create LocalClipboard: {}", e))?,
    );

    // Create shared clipboard change origin for write-back loop prevention.
    // This same Arc will be passed to InboundClipboardSyncWorker when inbound sync is added
    // (per D-09), so both sides share the same state for origin detection.
    let clipboard_change_origin: Arc<dyn uc_core::ports::ClipboardChangeOriginPort> =
        Arc::new(InMemoryClipboardChangeOrigin::new());

    let clipboard_change_handler = Arc::new(DaemonClipboardChangeHandler::new(
        runtime.clone(),
        event_tx.clone(),
        clipboard_change_origin.clone(),
    ));
    let clipboard_watcher = Arc::new(ClipboardWatcherWorker::new(
        local_clipboard,
        clipboard_change_handler,
    ));

    // InboundClipboardSyncWorker receives clipboard messages from peers and applies them
    // via SyncInboundClipboardUseCase (Full mode). Emits clipboard.new_content WS events
    // only for Applied { entry_id: Some(_) } outcomes.
    // Shares clipboard_change_origin with DaemonClipboardChangeHandler to prevent write-back loops.
    let inbound_clipboard_sync = Arc::new(InboundClipboardSyncWorker::new(
        runtime.clone(),
        event_tx.clone(),
        clipboard_change_origin.clone(), // SAME Arc as DaemonClipboardChangeHandler
        Some(file_cache_dir),
        Some(file_transfer_orchestrator),
    ));

    // 5. Assemble services vec — typed services erased to Arc<dyn DaemonService> (per D-05).
    //    Order matters for shutdown (reversed): peer-monitor and pairing-host stop last.
    let services: Vec<Arc<dyn DaemonService>> = vec![
        Arc::clone(&clipboard_watcher) as Arc<dyn DaemonService>,
        Arc::clone(&inbound_clipboard_sync) as Arc<dyn DaemonService>,
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
