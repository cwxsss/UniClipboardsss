//! Reusable daemon entry point.
//!
//! Contains the full composition root extracted from `main.rs` so that both the
//! standalone `uniclipboard-daemon` binary and the `uniclipboard-cli daemon`
//! subcommand can start an identical daemon process.

use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
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
use uc_bootstrap::build_non_gui_runtime_with_emitter;
use uc_bootstrap::builders::build_daemon_app;
use uc_bootstrap::BlobProcessingPorts;
use uc_core::ports::SystemClipboardPort;

use crate::api::types::DaemonWsEvent;
use crate::app::DaemonApp;
use crate::search::coordinator::SearchCoordinatorService;
use crate::service::DaemonService;
use crate::service::ServiceHealth;
use crate::state::{DaemonServiceSnapshot, RuntimeState};
use crate::workers::clipboard_watcher::{ClipboardWatcherWorker, DaemonClipboardChangeHandler};
use crate::workers::file_sync_orchestrator::FileSyncOrchestratorWorker;
use crate::workers::inbound_clipboard_sync::InboundClipboardSyncWorker;
use crate::workers::peer_keepalive::PeerKeepAliveWorker;
use uc_platform::clipboard::LocalClipboard;

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

    let runtime = Arc::new(
        build_non_gui_runtime_with_emitter(ctx.deps, ctx.storage_paths.clone(), emitter_cell)?
            .with_clipboard_write_coordinator(clipboard_write_coordinator.clone()),
    );

    // 1. Create the shared broadcast channel for WebSocket events.
    //    All services that emit WS events write to this same sender.
    let (event_tx, _) = broadcast::channel::<DaemonWsEvent>(64);

    // Build typed workers that don't depend on encryption state first.
    let local_clipboard: Arc<dyn SystemClipboardPort> = Arc::new(
        LocalClipboard::new()
            .map_err(|e| anyhow::anyhow!("failed to create LocalClipboard: {}", e))?,
    );

    // Reuse the runtime's clipboard change origin so restore commands, watcher,
    // inbound sync, and file restore all observe the same guard state.
    let clipboard_change_origin = runtime
        .wiring_deps()
        .clipboard
        .clipboard_change_origin
        .clone();

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
        runtime.wiring_deps().clipboard.clipboard_entry_repo.clone(),
        runtime.wiring_deps().clipboard.clipboard_event_repo.clone(),
        runtime
            .wiring_deps()
            .clipboard
            .representation_policy
            .clone(),
        runtime
            .wiring_deps()
            .clipboard
            .representation_normalizer
            .clone(),
        runtime.wiring_deps().device.device_identity.clone(),
        runtime.wiring_deps().clipboard.representation_cache.clone(),
        runtime.wiring_deps().clipboard.spool_queue.clone(),
    ));
    let blob_materializer = Arc::new(FileCacheBlobMaterializer::new(
        blob_transfer_facade.clone(),
        file_cache_dir.clone(),
    ));
    let apply_inbound_uc = Arc::new(
        ApplyInboundClipboardUseCase::new(
            runtime.wiring_deps().clipboard.clipboard_entry_repo.clone(),
            Arc::clone(&apply_inbound_capture_uc) as Arc<dyn ApplyInboundCapture>,
            Arc::clone(&clipboard_write_coordinator) as Arc<dyn ApplyInboundWrite>,
        )
        .with_blob_materializer(blob_materializer),
    );
    let inbound_clipboard_facade = Arc::new(InboundClipboardFacade::new(apply_inbound_uc));
    let clipboard_capture_facade = Arc::new(ClipboardCaptureFacade::new(apply_inbound_capture_uc));
    let clipboard_live_index_facade = Arc::new(ClipboardLiveIndexFacade::new(Arc::new(
        ClipboardLiveIndexer::new(ClipboardLiveIndexDeps {
            clipboard_entry_repo: runtime.wiring_deps().clipboard.clipboard_entry_repo.clone(),
            representation_policy: runtime
                .wiring_deps()
                .clipboard
                .representation_policy
                .clone(),
            search_key_derivation: runtime.wiring_deps().search.search_key_derivation.clone(),
            search_pipeline: runtime.wiring_deps().search.search_pipeline.clone(),
            search_index: runtime.wiring_deps().search.search_index.clone(),
        }),
    )));
    let clipboard_outbound_facade = Arc::new(ClipboardOutboundFacade::new(Arc::new(
        ClipboardOutboundDispatcher::new(ClipboardOutboundDeps {
            settings: runtime.wiring_deps().settings.clone(),
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

    // Keep iroh magicsock paths warm so blob fetches don't cold-start 30s+
    // after being idle for a minute. Ticks `SpaceSetupFacade::refresh_presence`
    // every 25s (well under the ~60s QUIC idle timeout).
    let peer_keepalive_worker: Arc<dyn DaemonService> = Arc::new(PeerKeepAliveWorker::new(
        ctx.space_setup_assembly.facade.clone(),
    ));

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
    let task_registry = runtime.task_registry().clone();
    rt.spawn(async move {
        uc_bootstrap::spawn_blob_processing_tasks(background, blob_ports, &task_registry).await;
    });

    let should_defer_clipboard = gui_managed || !encryption_unlocked;

    // Construct the search coordinator before building the service snapshots.
    let search_deps = runtime.wiring_deps();
    let search_coordinator = Arc::new(SearchCoordinator::new(SearchCoordinatorDeps::new(
        search_deps.search.search_index.clone(),
        search_deps.search.search_key_derivation.clone(),
        search_deps.search.search_pipeline.clone(),
        search_deps.clipboard.clipboard_entry_repo.clone(),
        search_deps.clipboard.representation_repo.clone(),
        search_deps.clipboard.selection_repo.clone(),
    )));
    let search_coordinator_service = Arc::new(SearchCoordinatorService::new(
        Arc::clone(&search_coordinator),
        event_tx.clone(),
    ));

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
            name: "peer-keepalive".to_string(),
            health: if encryption_unlocked {
                ServiceHealth::Healthy
            } else {
                ServiceHealth::Stopped
            },
        },
        DaemonServiceSnapshot {
            name: "peer-monitor".to_string(),
            health: ServiceHealth::Healthy,
        },
        DaemonServiceSnapshot {
            name: "search-coordinator".to_string(),
            health: if should_defer_clipboard {
                ServiceHealth::Stopped
            } else {
                ServiceHealth::Healthy
            },
        },
    ];
    let state = Arc::new(RwLock::new(RuntimeState::new(initial_statuses)));

    let (services, deferred_services) = {
        let mut initial: Vec<Arc<dyn DaemonService>> =
            vec![Arc::clone(&file_sync_orchestrator_worker) as Arc<dyn DaemonService>];
        let mut deferred: Vec<Arc<dyn DaemonService>> = Vec::new();

        if should_defer_clipboard {
            deferred.push(Arc::clone(&clipboard_watcher) as Arc<dyn DaemonService>);
            deferred.push(Arc::clone(&inbound_clipboard_sync) as Arc<dyn DaemonService>);
            // Defer search coordinator alongside clipboard services when locked/GUI-managed
            deferred.push(Arc::clone(&search_coordinator_service) as Arc<dyn DaemonService>);
        } else {
            initial.push(Arc::clone(&clipboard_watcher) as Arc<dyn DaemonService>);
            initial.push(Arc::clone(&inbound_clipboard_sync) as Arc<dyn DaemonService>);
            initial.push(Arc::clone(&search_coordinator_service) as Arc<dyn DaemonService>);
        }

        if encryption_unlocked {
            initial.push(Arc::clone(&peer_keepalive_worker) as Arc<dyn DaemonService>);
        } else {
            deferred.push(peer_keepalive_worker);
        }

        (initial, deferred)
    };
    let deferred_notify_opt = if deferred_services.is_empty() {
        None
    } else {
        Some(deferred_ready_notify.clone())
    };

    // Slice4 P3 T3.3 — clone the new SpaceSetupFacade Arc + resolve the
    // sponsor device id (stable for the daemon's lifetime) so the
    // pairing-completion forwarder doesn't need to pull
    // `DeviceIdentityPort` at event time. The facade itself is moved
    // (along with the rest of the assembly) into the post-`daemon.run()`
    // shutdown closure below; this clone keeps the api_state + forwarder
    // alive throughout `run()`.
    let space_setup_facade_for_api = ctx.space_setup_assembly.facade.clone();
    let member_roster_facade_for_api = ctx.space_setup_assembly.roster.clone();
    let local_device_id = runtime
        .wiring_deps()
        .device
        .device_identity
        .current_device_id()
        .to_string();

    let daemon = DaemonApp::new_with_deferred(
        services,
        runtime.clone(),
        state,
        event_tx,
        Some(ctx.space_access_facade),
        encryption_unlocked,
        deferred_services,
        deferred_notify_opt,
        external_shutdown,
        Some(clipboard_capture_gate.clone()),
        Some(search_coordinator),
        Some(space_setup_facade_for_api),
        Some(member_roster_facade_for_api),
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
    let unlock_runtime = runtime.clone();
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
                let settings = unlock_runtime
                    .wiring_deps()
                    .settings
                    .load()
                    .await
                    .unwrap_or_default();
                settings.security.auto_unlock_enabled
            } else {
                true
            };

            let unlocked =
                match crate::app::recover_encryption_session(&unlock_runtime, auto_unlock_enabled)
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
