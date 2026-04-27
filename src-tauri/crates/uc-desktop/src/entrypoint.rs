//! 可复用的 daemon 入口。
//!
//! 独立的 `uniclipboard-daemon` 二进制和 `uniclipboard-cli daemon` 子命令
//! 都通过这里启动同一套 daemon 进程。

use std::sync::Arc;

use tokio::sync::broadcast;
use uc_bootstrap::build_non_gui_bundle;
use uc_bootstrap::builders::build_daemon_app;
use uc_bootstrap::{BlobProcessingPorts, NonGuiBundle};

use crate::daemon::app_assembly::{build_daemon_app_instance, DaemonAppAssemblyInput};
use crate::daemon::app_facade_assembly::{build_daemon_app_facade, DaemonAppFacadeAssemblyInput};
use crate::daemon::background_tasks::spawn_daemon_background_tasks;
use crate::daemon::run_mode::DaemonRunMode;
use crate::daemon::runtime_assembly::{build_daemon_runtime_workers, DaemonRuntimeAssemblyInput};
use crate::daemon::search_assembly::build_daemon_search_assembly;
use crate::daemon::service_plan::{DaemonServicePlan, DaemonServicePlanInput};
use crate::daemon::shutdown::build_external_shutdown_token;
use crate::daemon::startup_recovery::{spawn_startup_recovery, StartupRecoveryInput};
use crate::service::DaemonService;
use uc_webserver::api::types::DaemonWsEvent;

/// 运行 daemon 进程。
///
/// 这里是桌面 daemon 模式的组装入口。
pub fn run(run_mode: DaemonRunMode) -> anyhow::Result<()> {
    let external_shutdown = build_external_shutdown_token(run_mode);

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

    // 剪贴板采集开关：独立和常驻模式默认打开，GUI sidecar 模式等 GUI
    // 发出 ready 后再打开。
    let clipboard_capture_gate = Arc::new(std::sync::atomic::AtomicBool::new(
        !run_mode.waits_for_gui_ready(),
    ));

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

    // 启动时先按“未解锁”处理，把恢复动作放到 HTTP 监听启动后的后台任务里。
    // macOS 钥匙串冷启动可能阻塞数秒，不能卡住 GUI 的健康检查。
    let encryption_unlocked = false;

    spawn_daemon_background_tasks(&rt, background, blob_ports, task_registry.clone());

    let search_assembly = build_daemon_search_assembly(&deps, event_tx.clone());

    let service_plan = DaemonServicePlan::build(DaemonServicePlanInput {
        run_mode,
        encryption_unlocked,
        file_sync_orchestrator: Arc::clone(&runtime_workers.file_sync_orchestrator)
            as Arc<dyn DaemonService>,
        clipboard_watcher: Arc::clone(&runtime_workers.clipboard_watcher) as Arc<dyn DaemonService>,
        inbound_clipboard_sync: Arc::clone(&runtime_workers.inbound_clipboard_sync)
            as Arc<dyn DaemonService>,
        search_coordinator: Arc::clone(&search_assembly.service) as Arc<dyn DaemonService>,
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

    let storage_paths_for_daemon = storage_paths.clone();
    let app_facade = build_daemon_app_facade(DaemonAppFacadeAssemblyInput {
        deps: &deps,
        storage_paths: &storage_paths_for_daemon,
        lifecycle_status: lifecycle_status.clone(),
        space_setup: space_setup_facade_for_api.clone(),
        member_roster: member_roster_facade_for_api.clone(),
        clipboard_sync: clipboard_sync_facade.clone(),
        blob_transfer: blob_transfer_facade.clone(),
        clipboard_write_coordinator: clipboard_write_coordinator.clone(),
        clipboard_integration_mode,
        search_coordinator: Arc::clone(&search_assembly.coordinator),
    });
    let unlock_app_facade = Arc::clone(&app_facade);

    let daemon = build_daemon_app_instance(DaemonAppAssemblyInput {
        service_plan,
        app_facade: Arc::clone(&app_facade),
        storage_paths: storage_paths_for_daemon,
        emitter_cell: emitter_cell.clone(),
        event_tx,
        encryption_unlocked,
        deferred_ready_notify: deferred_ready_notify.clone(),
        external_shutdown,
        clipboard_capture_gate: clipboard_capture_gate.clone(),
        local_device_id,
    });

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
            run_mode,
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
