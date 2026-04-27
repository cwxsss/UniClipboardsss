//! Reusable daemon entry point.
//!
//! Contains the full composition root extracted from `main.rs` so that both the
//! standalone `uniclipboard-daemon` binary and the `uniclipboard-cli daemon`
//! subcommand can start an identical daemon process.

use std::sync::Arc;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uc_application::facade::{SearchCoordinator, SearchCoordinatorDeps};
use uc_bootstrap::build_non_gui_bundle;
use uc_bootstrap::builders::build_daemon_app;
use uc_bootstrap::{
    build_app_facade_from_deps, AppFacadeAssemblyOptions, BlobProcessingPorts,
    ClipboardRestoreAssembly, NonGuiBundle,
};

use crate::app::DaemonApp;
use crate::daemon::runtime_assembly::{build_daemon_runtime_workers, DaemonRuntimeAssemblyInput};
use crate::daemon::service_plan::{DaemonServicePlan, DaemonServicePlanInput};
use crate::daemon::startup_recovery::{spawn_startup_recovery, StartupRecoveryInput};
use crate::search::coordinator::SearchCoordinatorService;
use crate::service::DaemonService;
use crate::workers::peer_keepalive::PeerKeepAliveWorker;
use uc_webserver::api::types::DaemonWsEvent;

/// Run the daemon process. This is the composition root.
///
/// `gui_managed` mirrors the `--gui-managed` flag: when true, the daemon
/// monitors stdin for EOF (parent GUI exit) and defers clipboard capture
/// until the GUI signals readiness.
pub fn run(gui_managed: bool) -> anyhow::Result<()> {
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

    // Build the daemon's tokio runtime FIRST. Everything async in the
    // daemon's lifetime — iroh Endpoint::bind (inside build_daemon_app),
    // recover_encryption_session, try_resume_session + refresh_presence,
    // and finally daemon.run() — MUST share this single long-lived
    // runtime. `Endpoint::bind` spawns magicsock / relay / STUN actors
    // via `tokio::spawn`; if they run on a short-lived rt that drops
    // after construction, the Endpoint becomes a zombie and `connect()`
    // returns "Unable to connect to remote" instantly while `accept`
    // sees no inbound traffic.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // build_daemon_app() calls build_core() which inits tracing + wires
    // deps, then awaits `build_space_setup_assembly` which binds iroh.
    let ctx = rt.block_on(build_daemon_app())?;
    // Extract file_cache_dir, file_transfer_orchestrator, clipboard_write_coordinator,
    // and emitter_cell before ctx is consumed by runtime construction.
    let file_cache_dir = ctx.storage_paths.file_cache_dir.clone();
    let file_transfer_lifecycle = ctx.background.file_transfer_lifecycle.clone();
    let clipboard_write_coordinator = ctx.background.clipboard_write_coordinator.clone();
    let emitter_cell = ctx.emitter_cell.clone();

    // Extract blob processing ports before ctx.deps is moved.
    let blob_ports = BlobProcessingPorts::from_app_deps(&ctx.deps);
    let background = ctx.background;

    // Create Notify for deferred service startup. After Slice4 P3 T3.4 the
    // legacy SetupCompletionEmitter is gone; deferred services now wait
    // exclusively on the `/lifecycle/ready` API endpoint (GUI signals unlock).
    let deferred_ready_notify = Arc::new(tokio::sync::Notify::new());

    let NonGuiBundle {
        deps,
        storage_paths,
        emitter_cell: _bundle_emitter_cell,
        lifecycle_status,
        task_registry,
        clipboard_integration_mode,
    } = build_non_gui_bundle(ctx.deps, ctx.storage_paths.clone(), emitter_cell.clone())?;
    // Settings + clipboard_change_origin are lifted out so the spawn
    // closures further down can take owned Arc clones without holding
    // a `&deps` borrow across `await`.
    let settings_port = deps.settings.clone();

    // 1. Create the shared broadcast channel for WebSocket events.
    //    All services that emit WS events write to this same sender.
    let (event_tx, _) = broadcast::channel::<DaemonWsEvent>(64);

    // Clipboard capture gate: controls whether clipboard changes are processed.
    // In standalone CLI mode, capture is always enabled.
    // In GUI-managed mode, capture is disabled until the GUI signals readiness.
    let clipboard_capture_gate = Arc::new(std::sync::atomic::AtomicBool::new(!gui_managed));

    // Slice 2 Phase 3 · T6 — pull clipboard_sync_facade out of the
    // assembly before we move it into `space_setup_assembly` for
    // shutdown. Feeds both the outbound watcher (T7) and the inbound
    // worker (T8).
    let clipboard_sync_facade = ctx.clipboard_sync_facade.clone();
    let blob_transfer_facade = ctx.space_setup_assembly.blob.clone();

    let runtime_workers = build_daemon_runtime_workers(DaemonRuntimeAssemblyInput {
        deps: &deps,
        event_tx: event_tx.clone(),
        clipboard_capture_gate: clipboard_capture_gate.clone(),
        clipboard_sync_facade: clipboard_sync_facade.clone(),
        blob_transfer_facade: blob_transfer_facade.clone(),
        file_cache_dir: file_cache_dir.clone(),
        file_transfer_lifecycle,
        clipboard_write_coordinator: clipboard_write_coordinator.clone(),
    })?;

    // `PeerKeepAliveWorker` is constructed AFTER `AppFacade` assembly below
    // so it can hold `Arc<AppFacade>` and reach `space_setup` via the
    // single AppFacade entry (see app_facade.rs module docs / AGENTS §11.4).
    // The service-list assembly below leaves a `mut` slot for it, and the
    // post-assembly block inserts it into `services` or `deferred_services`
    // based on `encryption_unlocked`.

    // Phase 67 (revised): encryption recovery + slice 2/3 try_resume_session
    // run in a background task spawned below inside `rt.block_on`, AFTER
    // `daemon.run()` brings up the HTTP listener. macOS keychain calls
    // routinely block 5–7 s on cold start; doing them on the startup
    // critical path used to push daemon-ready past the GUI's 8 s health
    // timeout. Treating the daemon as "locked until proven unlocked"
    // keeps clipboard + keepalive in the deferred bucket; on success the
    // background task auto-triggers them in CLI mode (GUI mode still
    // waits for an explicit `/lifecycle/ready`).
    let encryption_unlocked = false;

    // Start background clipboard processing tasks.
    let task_registry = task_registry.clone();
    rt.spawn(async move {
        uc_bootstrap::spawn_blob_processing_tasks(background, blob_ports, &task_registry).await;
    });

    // Construct the search coordinator before building the service snapshots.
    let search_coordinator = Arc::new(SearchCoordinator::new(SearchCoordinatorDeps::new(
        deps.search.search_index.clone(),
        deps.search.search_key_derivation.clone(),
        deps.search.search_pipeline.clone(),
        deps.clipboard.clipboard_entry_repo.clone(),
        deps.clipboard.representation_repo.clone(),
        deps.clipboard.selection_repo.clone(),
    )));
    let search_coordinator_service = Arc::new(SearchCoordinatorService::new(
        Arc::clone(&search_coordinator),
        event_tx.clone(),
    ));

    let mut service_plan = DaemonServicePlan::build(DaemonServicePlanInput {
        gui_managed,
        encryption_unlocked,
        file_sync_orchestrator: Arc::clone(&runtime_workers.file_sync_orchestrator)
            as Arc<dyn DaemonService>,
        clipboard_watcher: Arc::clone(&runtime_workers.clipboard_watcher) as Arc<dyn DaemonService>,
        inbound_clipboard_sync: Arc::clone(&runtime_workers.inbound_clipboard_sync)
            as Arc<dyn DaemonService>,
        search_coordinator: Arc::clone(&search_coordinator_service) as Arc<dyn DaemonService>,
    });

    // Slice4 P3 T3.3 — clone the new SpaceSetupFacade Arc + resolve the
    // sponsor device id (stable for the daemon's lifetime) so the
    // pairing-completion forwarder doesn't need to pull
    // `DeviceIdentityPort` at event time. The facade itself is moved
    // (along with the rest of the assembly) into the post-`daemon.run()`
    // shutdown closure below; this clone keeps the api_state + forwarder
    // alive throughout `run()`.
    let space_setup_facade_for_api = ctx.space_setup_assembly.facade.clone();
    let member_roster_facade_for_api = ctx.space_setup_assembly.roster.clone();
    let local_device_id = deps.device.device_identity.current_device_id().to_string();

    // Assemble AppFacade — this is the single outward-facing entry point
    // the daemon (and all HTTP handlers) reach through. Common facade
    // construction lives in uc-bootstrap so desktop modes do not each hand-roll
    // the same sub-facade graph.
    let storage_paths_for_daemon = storage_paths.clone();
    let app_facade = build_app_facade_from_deps(
        &deps,
        &storage_paths_for_daemon,
        lifecycle_status.clone(),
        AppFacadeAssemblyOptions {
            space_setup: Some(space_setup_facade_for_api.clone()),
            member_roster: Some(member_roster_facade_for_api.clone()),
            clipboard_sync: Some(clipboard_sync_facade.clone()),
            blob_transfer: Some(blob_transfer_facade.clone()),
            clipboard_restore: Some(ClipboardRestoreAssembly {
                write_coordinator: clipboard_write_coordinator.clone(),
                integration_mode: clipboard_integration_mode,
            }),
            search_coordinator: Some(Arc::clone(&search_coordinator)),
        },
    );
    let unlock_app_facade = Arc::clone(&app_facade);

    // Keep iroh magicsock paths warm so blob fetches don't cold-start 30s+
    // after being idle for a minute. Ticks `space_setup.refresh_presence`
    // every 25s (well under the ~60s QUIC idle timeout). Constructed here
    // (post-AppFacade assembly) so it can hold `Arc<AppFacade>` and reach
    // `space_setup` via the single AppFacade entry — see app_facade.rs
    // module docs / AGENTS §11.4.
    let peer_keepalive_worker: Arc<dyn DaemonService> =
        Arc::new(PeerKeepAliveWorker::new(Arc::clone(&app_facade)));
    service_plan.add_peer_keepalive(encryption_unlocked, peer_keepalive_worker);
    let deferred_notify_opt = service_plan.deferred_ready_notify(deferred_ready_notify.clone());

    let daemon = DaemonApp::new_with_deferred(
        service_plan.services,
        Arc::clone(&app_facade),
        storage_paths_for_daemon,
        emitter_cell.clone(),
        service_plan.state,
        event_tx,
        encryption_unlocked,
        service_plan.deferred_services,
        deferred_notify_opt,
        external_shutdown,
        Some(clipboard_capture_gate.clone()),
        Some(local_device_id),
    );

    // Slice 2 Phase 3 · T6 — move the iroh-stack assembly into the
    // runtime closure so its shutdown runs AFTER daemon.run() returns
    // (graceful or signal-driven) but BEFORE the runtime itself drops.
    // `SpaceSetupAssembly::shutdown` aborts the ingest loop + tears
    // down the iroh router, emitting `CONNECTION_CLOSE` to any live
    // peer. Without this, peers see TCP RST / QUIC timeout instead of
    // a clean close.
    let space_setup_assembly = ctx.space_setup_assembly;
    let unlock_settings = settings_port.clone();
    let unlock_facade = space_setup_assembly.facade.clone();
    let unlock_notify = deferred_ready_notify.clone();
    let unlock_gate = clipboard_capture_gate.clone();
    rt.block_on(async move {
        spawn_startup_recovery(StartupRecoveryInput {
            gui_managed,
            app_facade: unlock_app_facade,
            settings: unlock_settings,
            space_setup: unlock_facade,
            deferred_ready_notify: unlock_notify,
            clipboard_capture_gate: unlock_gate,
        });

        let res = daemon.run().await;
        space_setup_assembly.shutdown().await;
        res
    })
}
