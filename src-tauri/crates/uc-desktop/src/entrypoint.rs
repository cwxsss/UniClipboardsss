//! Reusable daemon entry point.
//!
//! Contains the full composition root extracted from `main.rs` so that both the
//! standalone `uniclipboard-daemon` binary and the `uniclipboard-cli daemon`
//! subcommand can start an identical daemon process.

use std::sync::Arc;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use uc_application::clipboard_capture::CaptureClipboardUseCase;
use uc_application::facade::{
    ClipboardCaptureFacade, ClipboardLiveIndexDeps, ClipboardLiveIndexFacade, ClipboardLiveIndexer,
    ClipboardOutboundDeps, ClipboardOutboundDispatcher, ClipboardOutboundFacade,
    InboundClipboardFacade, SearchCoordinator, SearchCoordinatorDeps,
};
use uc_application::{
    ApplyInboundClipboardUseCase, FileCacheBlobMaterializer, InboundCapture as ApplyInboundCapture,
    InboundWrite as ApplyInboundWrite,
};
use uc_bootstrap::build_non_gui_bundle;
use uc_bootstrap::builders::build_daemon_app;
use uc_bootstrap::{
    build_app_facade_from_deps, AppFacadeAssemblyOptions, BlobProcessingPorts,
    ClipboardRestoreAssembly, NonGuiBundle,
};
use uc_core::ports::SystemClipboardPort;

use crate::app::DaemonApp;
use crate::daemon::service_plan::{DaemonServicePlan, DaemonServicePlanInput};
use crate::search::coordinator::SearchCoordinatorService;
use crate::service::DaemonService;
use crate::workers::clipboard_watcher::{ClipboardWatcherWorker, DaemonClipboardChangeHandler};
use crate::workers::file_sync_orchestrator::FileSyncOrchestratorWorker;
use crate::workers::inbound_clipboard_sync::InboundClipboardSyncWorker;
use crate::workers::peer_keepalive::PeerKeepAliveWorker;
use uc_platform::clipboard::LocalClipboard;
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

    // Build typed workers that don't depend on encryption state first.
    let local_clipboard: Arc<dyn SystemClipboardPort> = Arc::new(
        LocalClipboard::new()
            .map_err(|e| anyhow::anyhow!("failed to create LocalClipboard: {}", e))?,
    );

    // Reuse the bootstrap-built clipboard change origin so restore commands, watcher,
    // inbound sync, and file restore all observe the same guard state.
    let clipboard_change_origin = deps.clipboard.clipboard_change_origin.clone();

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

    // Slice 2 Phase 3 · T6 — build `ApplyInboundClipboardUseCase` from
    // runtime wiring deps. `CaptureClipboardUseCase` (migrated to
    // uc-application in T0a) gets wrapped as `Arc<dyn InboundCapture>`
    // via its blanket impl; `ClipboardWriteCoordinator` (T0b) likewise
    // wraps as `Arc<dyn InboundWrite>`. The coordinator shared here is
    // the exact same Arc the watcher reads `clipboard_change_origin`
    // from, so RemotePush guards register and consume on one cache.
    let apply_inbound_capture_uc = Arc::new(CaptureClipboardUseCase::new(
        deps.clipboard.clipboard_entry_repo.clone(),
        deps.clipboard.clipboard_event_repo.clone(),
        deps.clipboard.representation_policy.clone(),
        deps.clipboard.representation_normalizer.clone(),
        deps.device.device_identity.clone(),
        deps.clipboard.representation_cache.clone(),
        deps.clipboard.spool_queue.clone(),
    ));
    let blob_materializer = Arc::new(FileCacheBlobMaterializer::new(
        blob_transfer_facade.clone(),
        file_cache_dir.clone(),
    ));
    let apply_inbound_uc = Arc::new(
        ApplyInboundClipboardUseCase::new(
            deps.clipboard.clipboard_entry_repo.clone(),
            Arc::clone(&apply_inbound_capture_uc) as Arc<dyn ApplyInboundCapture>,
            Arc::clone(&clipboard_write_coordinator) as Arc<dyn ApplyInboundWrite>,
        )
        .with_blob_materializer(blob_materializer),
    );
    let inbound_clipboard_facade = Arc::new(InboundClipboardFacade::new(apply_inbound_uc));
    let clipboard_capture_facade = Arc::new(ClipboardCaptureFacade::new(apply_inbound_capture_uc));
    let clipboard_live_index_facade = Arc::new(ClipboardLiveIndexFacade::new(Arc::new(
        ClipboardLiveIndexer::new(ClipboardLiveIndexDeps {
            clipboard_entry_repo: deps.clipboard.clipboard_entry_repo.clone(),
            representation_policy: deps.clipboard.representation_policy.clone(),
            search_key_derivation: deps.search.search_key_derivation.clone(),
            search_pipeline: deps.search.search_pipeline.clone(),
            search_index: deps.search.search_index.clone(),
        }),
    )));
    let clipboard_outbound_facade = Arc::new(ClipboardOutboundFacade::new(Arc::new(
        ClipboardOutboundDispatcher::new(ClipboardOutboundDeps {
            settings: deps.settings.clone(),
            clipboard_sync: clipboard_sync_facade.clone(),
            blob_transfer: blob_transfer_facade.clone(),
        }),
    )));

    let clipboard_change_handler = Arc::new(DaemonClipboardChangeHandler::new(
        event_tx.clone(),
        clipboard_change_origin.clone(),
        clipboard_capture_gate.clone(),
        clipboard_capture_facade,
        clipboard_live_index_facade,
        clipboard_outbound_facade,
    ));
    let clipboard_watcher = Arc::new(ClipboardWatcherWorker::new(
        local_clipboard.clone(),
        clipboard_change_handler,
    ));

    let inbound_clipboard_sync = Arc::new(InboundClipboardSyncWorker::new(
        clipboard_sync_facade.clone(),
        inbound_clipboard_facade,
        event_tx.clone(),
    ));

    let file_sync_orchestrator_worker =
        Arc::new(FileSyncOrchestratorWorker::new(file_transfer_lifecycle));

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
        file_sync_orchestrator: Arc::clone(&file_sync_orchestrator_worker)
            as Arc<dyn DaemonService>,
        clipboard_watcher: Arc::clone(&clipboard_watcher) as Arc<dyn DaemonService>,
        inbound_clipboard_sync: Arc::clone(&inbound_clipboard_sync) as Arc<dyn DaemonService>,
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
        // Background unlock task. Combines the legacy
        // `recover_encryption_session` keychain path with the slice 2/3
        // `space_setup_facade.try_resume_session` + `refresh_presence` priming
        // that used to run synchronously here. Both touch the macOS keychain
        // and routinely take seconds; running them off the critical path lets
        // the HTTP listener come up first. Errors are logged, never fatal —
        // GUI / CLI can recover via manual `/space/unlock` + `/lifecycle/ready`.
        tokio::spawn(async move {
            use tracing::{info_span, Instrument};

            let auto_unlock_enabled = if gui_managed {
                let settings = unlock_settings.load().await.unwrap_or_default();
                settings.security.auto_unlock_enabled
            } else {
                true
            };

            let unlocked = match crate::app::recover_encryption_session(
                &unlock_app_facade,
                auto_unlock_enabled,
            )
            .instrument(info_span!("daemon.startup.recover_encryption_session"))
            .await
            {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "background unlock: recover_encryption_session failed"
                    );
                    false
                }
            };

            // Slice 2 Phase 3 fix — resume the Slice 1+ Space session and
            // prime peer reachability. Independent of the legacy unlock path
            // above; both touch the keychain but for different `space_id`s.
            // `Ok(false)` means the profile has no space yet (pre-`init/join`).
            match unlock_facade.try_resume_session().await {
                Ok(true) => {
                    if let Err(e) = unlock_facade.refresh_presence().await {
                        tracing::warn!(error = %e, "background unlock: presence probe failed");
                    }
                }
                Ok(false) => {
                    tracing::info!(
                        "background unlock: no space on this profile — skipping resume/probe"
                    );
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "background unlock: try_resume_session failed");
                }
            }

            // CLI mode: a successful keychain unlock auto-triggers the
            // deferred clipboard / keepalive services. GUI mode keeps waiting
            // for `/lifecycle/ready` so the GUI controls when capture begins.
            if !gui_managed && unlocked {
                unlock_gate.store(true, std::sync::atomic::Ordering::SeqCst);
                unlock_notify.notify_one();
                tracing::info!("background unlock: CLI mode auto-triggered deferred services");
            }
        });

        let res = daemon.run().await;
        space_setup_assembly.shutdown().await;
        res
    })
}
