//! UniClipboard daemon binary entry point.
//!
//! Bootstraps via `build_daemon_app()` for config/paths, creates services,
//! and runs `DaemonApp` in a tokio runtime.
//!
//! This is the composition root: typed services are built here, then erased
//! to `Arc<dyn DaemonService>` for uniform lifecycle management by DaemonApp.

use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use tokio_util::sync::CancellationToken;
use uc_app::usecases::LoggingLifecycleEventEmitter;
use uc_app::usecases::SessionReadyEmitter;
use uc_bootstrap::assembly::SetupAssemblyPorts;
use uc_bootstrap::build_non_gui_runtime_with_emitter;
use uc_bootstrap::builders::build_daemon_app;
use uc_bootstrap::BlobProcessingPorts;
use uc_core::ports::SystemClipboardPort;
use uc_daemon::api::types::DaemonWsEvent;
use uc_daemon::app::{DaemonApp, SetupCompletionEmitter};
use uc_daemon::pairing::host::DaemonPairingHost;
use uc_daemon::peers::monitor::PeerMonitor;
use uc_daemon::service::DaemonService;
use uc_daemon::service::ServiceHealth;
use uc_daemon::socket::resolve_daemon_socket_path;
use uc_daemon::state::{DaemonServiceSnapshot, RuntimeState};
use uc_daemon::workers::clipboard_watcher::{ClipboardWatcherWorker, DaemonClipboardChangeHandler};
use uc_daemon::workers::file_sync_orchestrator::FileSyncOrchestratorWorker;
use uc_daemon::workers::inbound_clipboard_sync::InboundClipboardSyncWorker;
use uc_daemon::workers::peer_discovery::PeerDiscoveryWorker;
use uc_infra::clipboard::InMemoryClipboardChangeOrigin;
use uc_platform::clipboard::LocalClipboard;

fn main() -> anyhow::Result<()> {
    let gui_managed = std::env::args().any(|arg| arg == "--gui-managed");

    // When launched with --gui-managed, the parent GUI process keeps our stdin pipe open.
    // If the parent exits (normally, crash, or SIGKILL), the pipe closes and we detect EOF.
    // This token fires on EOF, triggering graceful daemon shutdown via DaemonApp's select loop.
    let external_shutdown = if gui_managed {
        let token = CancellationToken::new();
        let token_clone = token.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = [0u8; 1];
            // Blocks until stdin is closed (parent process gone)
            let _ = std::io::stdin().read(&mut buf);
            token_clone.cancel();
        });
        Some(token)
    } else {
        None
    };

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
    // by build_non_gui_runtime_with_emitter (which moves ctx.deps).
    let file_cache_dir = ctx.storage_paths.file_cache_dir.clone();
    let file_transfer_orchestrator = ctx.background.file_transfer_orchestrator.clone();

    // Extract blob processing ports before ctx.deps is moved.
    let blob_ports = BlobProcessingPorts::from_app_deps(&ctx.deps);
    let background = ctx.background;

    // Create Notify for deferred service startup.
    // This is triggered by either:
    // - SetupCompletionEmitter (setup flow completes on uninitialized device)
    // - /lifecycle/ready API endpoint (GUI signals unlock)
    let deferred_ready_notify = Arc::new(tokio::sync::Notify::new());
    let setup_completion_emitter: Arc<dyn SessionReadyEmitter> =
        Arc::new(SetupCompletionEmitter::new(deferred_ready_notify.clone()));

    let runtime = Arc::new(build_non_gui_runtime_with_emitter(
        ctx.deps,
        ctx.storage_paths.clone(),
        setup_ports,
        setup_completion_emitter,
    )?);

    let socket_path = resolve_daemon_socket_path();

    // 1. Create the shared broadcast channel for WebSocket events.
    //    All services that emit WS events write to this same sender.
    let (event_tx, _) = broadcast::channel::<DaemonWsEvent>(64);

    // 3. Build typed PairingHost (per D-07: construction before DaemonApp).
    //    Typed Arc is kept for DaemonApiState so HTTP routes retain typed access (PH56-04).
    // NOTE: state is built AFTER recover_encryption_session (see below), so we need
    // a temporary state here just for PairingHost construction.
    // Actually, PairingHost receives state by Arc and DaemonPairingHost::new() stores it.
    // We create state before PairingHost and update initial_statuses later — but state is
    // an Arc<RwLock<RuntimeState>> so the actual snapshot is set at construction.
    // Solution: build PairingHost before state; it takes state by Arc so we build state
    // first with placeholder, then rebuild after encryption check. HOWEVER, the simpler
    // approach: build the tokio runtime, run encryption check, THEN build state and services.

    // Build typed workers that don't depend on encryption state first.
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

    // Clipboard capture gate: controls whether clipboard changes are processed.
    // In standalone CLI mode, capture is always enabled.
    // In GUI-managed mode, capture is disabled until the GUI signals readiness
    // (after the user unlocks), preventing clipboard monitoring before unlock.
    let clipboard_capture_gate = Arc::new(std::sync::atomic::AtomicBool::new(!gui_managed));

    let clipboard_change_handler = Arc::new(DaemonClipboardChangeHandler::new(
        runtime.clone(),
        event_tx.clone(),
        clipboard_change_origin.clone(),
        clipboard_capture_gate.clone(),
    ));
    let clipboard_watcher = Arc::new(ClipboardWatcherWorker::new(
        local_clipboard.clone(), // clone — FileSyncOrchestratorWorker also needs this for clipboard restore
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
        Some(file_cache_dir.clone()),
        Some(file_transfer_orchestrator.clone()),
    ));

    // FileSyncOrchestratorWorker subscribes to network events for file transfer lifecycle.
    // Handles TransferProgress, FileTransferCompleted, FileTransferFailed events,
    // runs startup reconciliation, periodic timeout sweeps, and restores completed
    // files to the OS clipboard.
    let file_sync_orchestrator_worker = Arc::new(FileSyncOrchestratorWorker::new(
        file_transfer_orchestrator,
        daemon_network_events.clone(), // clone — PeerDiscoveryWorker also needs this
        local_clipboard.clone(),
        clipboard_change_origin.clone(),
        file_cache_dir,
        daemon_settings.clone(), // clone — PeerDiscoveryWorker also needs this
    ));

    // Build PeerDiscoveryWorker unconditionally (cheap to construct).
    // Whether it's included in initial services depends on encryption state (Phase 67 D-01/D-06).
    let peer_discovery_worker: Arc<dyn DaemonService> = Arc::new(PeerDiscoveryWorker::new(
        daemon_network_control,
        daemon_network_events,
        daemon_peer_directory,
        daemon_settings,
    ));

    // Use explicit runtime construction (consistent with uc-bootstrap pattern,
    // avoids potential conflicts with tracing init's internal runtime for Seq)
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // Phase 67: Recover encryption session BEFORE building services.
    // The result determines whether PeerDiscoveryWorker starts immediately or is deferred (per D-01, D-02, D-06, D-07).
    let encryption_unlocked = rt.block_on(async {
        use tracing::{info_span, Instrument};
        uc_daemon::app::recover_encryption_session(&runtime)
            .instrument(info_span!("daemon.startup.recover_encryption_session"))
            .await
    })?;

    // Start background clipboard processing tasks (SpoolScanner, SpoolerTask,
    // BackgroundBlobWorker, SpoolJanitor) via shared bootstrap function.
    // Uses the runtime's TaskRegistry for lifecycle management.
    let task_registry = runtime.task_registry().clone();
    rt.spawn(async move {
        uc_bootstrap::spawn_blob_processing_tasks(background, blob_ports, &task_registry).await;
    });

    // Clipboard services are deferred when:
    // - GUI-managed: wait for GUI unlock signal via /lifecycle/ready
    // - Encryption uninitialized: wait for setup completion
    let should_defer_clipboard = gui_managed || !encryption_unlocked;

    // Build initial_statuses AFTER encryption check so service health reflects actual state.
    let initial_statuses: Vec<DaemonServiceSnapshot> = vec![
        DaemonServiceSnapshot {
            name: "clipboard-watcher".to_string(),
            health: if should_defer_clipboard {
                ServiceHealth::Stopped
            } else {
                ServiceHealth::Healthy
            },
        },
        DaemonServiceSnapshot {
            name: "inbound-clipboard-sync".to_string(),
            health: if should_defer_clipboard {
                ServiceHealth::Stopped
            } else {
                ServiceHealth::Healthy
            },
        },
        DaemonServiceSnapshot {
            name: "file-sync-orchestrator".to_string(),
            health: ServiceHealth::Healthy,
        },
        DaemonServiceSnapshot {
            name: "peer-discovery".to_string(),
            health: if encryption_unlocked {
                ServiceHealth::Healthy
            } else {
                ServiceHealth::Stopped
            },
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

    // 2. Build typed PairingHost (per D-07: construction before DaemonApp).
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

    // Conditional service registration:
    // - Clipboard services are deferred when gui_managed or encryption uninitialized
    // - PeerDiscoveryWorker is deferred when encryption is uninitialized
    let (services, deferred_services) = {
        let mut initial: Vec<Arc<dyn DaemonService>> = vec![
            Arc::clone(&file_sync_orchestrator_worker) as Arc<dyn DaemonService>,
            Arc::clone(&pairing_host) as Arc<dyn DaemonService>,
            Arc::clone(&peer_monitor) as Arc<dyn DaemonService>,
        ];
        let mut deferred: Vec<Arc<dyn DaemonService>> = Vec::new();

        if should_defer_clipboard {
            deferred.push(Arc::clone(&clipboard_watcher) as Arc<dyn DaemonService>);
            deferred.push(Arc::clone(&inbound_clipboard_sync) as Arc<dyn DaemonService>);
        } else {
            initial.push(Arc::clone(&clipboard_watcher) as Arc<dyn DaemonService>);
            initial.push(Arc::clone(&inbound_clipboard_sync) as Arc<dyn DaemonService>);
        }

        if encryption_unlocked {
            initial.push(Arc::clone(&peer_discovery_worker) as Arc<dyn DaemonService>);
        } else {
            deferred.push(peer_discovery_worker);
        }

        (initial, deferred)
    };
    let deferred_notify_opt = if deferred_services.is_empty() {
        None
    } else {
        Some(deferred_ready_notify.clone())
    };

    // 5. Assemble and run daemon app.
    //    api_pairing_host retains typed access for HTTP routes.
    //    space_access_orchestrator is passed for API state wiring.
    let daemon = DaemonApp::new_with_deferred(
        services,
        runtime,
        state,
        event_tx,
        Some(pairing_host),
        Some(ctx.space_access_orchestrator),
        socket_path,
        encryption_unlocked,
        deferred_services,
        deferred_notify_opt,
        external_shutdown,
        Some(clipboard_capture_gate),
    );

    rt.block_on(daemon.run())
}
