//! 可复用的 daemon 入口。
//!
//! 独立的 `uniclipboard-daemon` 二进制和 `uniclipboard-cli daemon` 子命令
//! 都通过这里启动同一套 daemon 进程。

use std::sync::Arc;

use uc_bootstrap::build_non_gui_bundle;
use uc_bootstrap::builders::build_daemon_app;
use uc_bootstrap::{BlobProcessingPorts, NonGuiBundle};

use crate::daemon::app_assembly::{build_daemon_app_instance, DaemonAppAssemblyInput};
use crate::daemon::app_facade_assembly::{build_daemon_app_facade, DaemonAppFacadeAssemblyInput};
use crate::daemon::background_tasks::spawn_daemon_background_tasks;
use crate::daemon::run_loop::{run_daemon_until_shutdown, DaemonRunLoopInput};
use crate::daemon::run_mode::DaemonRunMode;
use crate::daemon::runtime_assembly::{build_daemon_runtime_workers, DaemonRuntimeAssemblyInput};
use crate::daemon::runtime_controls::build_daemon_runtime_controls;
use crate::daemon::search_assembly::build_daemon_search_assembly;
use crate::daemon::service_plan::{DaemonServicePlan, DaemonServicePlanInput};
use crate::daemon::shutdown::build_external_shutdown_token;
use crate::daemon::tokio_runtime::build_daemon_tokio_runtime;
use crate::service::DaemonService;

/// 运行 daemon 进程。
///
/// 这里是桌面 daemon 模式的组装入口。
pub fn run(run_mode: DaemonRunMode) -> anyhow::Result<()> {
    let external_shutdown = build_external_shutdown_token(run_mode);

    let rt = build_daemon_tokio_runtime()?;

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
    let runtime_controls = build_daemon_runtime_controls(run_mode);

    // Slice 2 Phase 3 · T6 — pull clipboard_sync_facade out of the
    // assembly before we move it into `space_setup_assembly` for
    // shutdown. Feeds both the outbound watcher (T7) and the inbound
    // worker (T8).
    let clipboard_sync_facade = ctx.clipboard_sync_facade.clone();
    let blob_transfer_facade = ctx.space_setup_assembly.blob.clone();

    let runtime_workers = build_daemon_runtime_workers(DaemonRuntimeAssemblyInput {
        deps: &deps,
        event_tx: runtime_controls.event_tx.clone(),
        clipboard_capture_gate: runtime_controls.clipboard_capture_gate.clone(),
        clipboard_sync_facade: clipboard_sync_facade.clone(),
        blob_transfer_facade: blob_transfer_facade.clone(),
        file_cache_dir: file_cache_dir.clone(),
        file_transfer_lifecycle,
        clipboard_write_coordinator: clipboard_write_coordinator.clone(),
    })?;

    spawn_daemon_background_tasks(&rt, background, blob_ports, task_registry.clone());

    let search_assembly = build_daemon_search_assembly(&deps, runtime_controls.event_tx.clone());

    let service_plan = DaemonServicePlan::build(DaemonServicePlanInput {
        run_mode,
        encryption_unlocked: runtime_controls.encryption_unlocked,
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
    let daemon = build_daemon_app_instance(DaemonAppAssemblyInput {
        service_plan,
        app_facade: Arc::clone(&app_facade),
        storage_paths: storage_paths_for_daemon,
        emitter_cell: emitter_cell.clone(),
        event_tx: runtime_controls.event_tx,
        encryption_unlocked: runtime_controls.encryption_unlocked,
        deferred_ready_notify: runtime_controls.deferred_ready_notify.clone(),
        external_shutdown,
        clipboard_capture_gate: runtime_controls.clipboard_capture_gate.clone(),
        local_device_id,
    });

    run_daemon_until_shutdown(
        &rt,
        DaemonRunLoopInput {
            run_mode,
            daemon,
            app_facade,
            settings: settings_port,
            space_setup_assembly: ctx.space_setup_assembly,
            deferred_ready_notify: runtime_controls.deferred_ready_notify,
            clipboard_capture_gate: runtime_controls.clipboard_capture_gate,
        },
    )
}
