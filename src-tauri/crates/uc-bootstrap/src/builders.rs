//! # Scene-Specific Builders
//!
//! Entry-point constructors for GUI, CLI, and daemon runtime modes.
//!
//! All three builders share a private `build_core()` helper that:
//! 1. Initializes tracing (idempotent)
//! 2. Resolves application config
//! 3. Wires all dependencies via `wire_dependencies`
//!
//! Each builder returns a context struct containing `AppDeps` (NOT `CoreRuntime`).
//! Callers construct `CoreRuntime` themselves with the appropriate emitter cell,
//! lifecycle status, and task registry.

use std::sync::Arc;

use uc_app::app_paths::AppPaths;
use uc_app::shared::host_event::HostEventEmitterPort;
use uc_app::AppDeps;
use uc_application::facade::ClipboardSyncFacade;
use uc_application::membership::usecases::AdmitMemberUseCase;
use uc_application::space_access::SpaceAccessFacade;
use uc_core::config::AppConfig;
use uc_platform::adapters::PairingRuntimeOwner;

use crate::assembly::{get_storage_paths, wire_dependencies, BackgroundRuntimeDeps};
use crate::space_setup::{build_space_setup_assembly, IrohNodeConfig, SpaceSetupAssembly};

/// Context for GUI entry point. Contains everything needed to construct
/// AppRuntime EXCEPT tauri::AppHandle. uc-tauri calls AppRuntime::with_setup()
/// using `deps` from this context -- NOT a prebuilt CoreRuntime.
///
/// [Codex Review R1] Returns AppDeps to preserve compatibility with
/// AppRuntime::with_setup() which builds CoreRuntime internally.
pub struct GuiBootstrapContext {
    pub deps: AppDeps,
    pub background: BackgroundRuntimeDeps,
    pub storage_paths: AppPaths,
    pub config: AppConfig,
}

/// Context for CLI entry point. AppDeps + config, no background workers.
/// Caller constructs CoreRuntime from deps as needed.
pub struct CliBootstrapContext {
    pub deps: AppDeps,
    pub config: AppConfig,
}

/// Context for daemon entry point. AppDeps + background deps,
/// workers not started. Caller constructs CoreRuntime and starts background workers.
pub struct DaemonBootstrapContext {
    pub deps: AppDeps,
    pub background: BackgroundRuntimeDeps,
    pub emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>,
    pub space_access_facade: Arc<SpaceAccessFacade>,
    pub storage_paths: AppPaths,
    pub config: AppConfig,
    /// Slice 2 Phase 3 · T5 — iroh-stack clipboard sync facade.
    /// Daemon's `DaemonClipboardChangeHandler` (T7) calls
    /// `clipboard_sync_facade.dispatch_snapshot(...)` instead of the
    /// deprecated libp2p `SyncOutboundClipboardUseCase`;
    /// `InboundClipboardSyncWorker` (T8) subscribes via
    /// `subscribe_inbound_notices()`.
    ///
    /// Same Arc as the one held by `space_setup_assembly.clipboard_sync` —
    /// kept here as a top-level field so daemon entrypoint code reads
    /// off `ctx.clipboard_sync_facade` directly without unwrapping the
    /// assembly.
    pub clipboard_sync_facade: Arc<ClipboardSyncFacade>,
    /// Slice 2 Phase 3 · T5 — full Slice 1+ assembly. Owns the iroh
    /// node, pairing/presence/clipboard handlers, and the auto-spawned
    /// ingest loop. Daemon shutdown calls `space_setup_assembly.shutdown()`
    /// to cleanly tear down router + abort ingest before the Tokio
    /// runtime exits.
    pub space_setup_assembly: SpaceSetupAssembly,
}

/// Shared core wiring used by all three builders.
/// Initializes tracing, resolves config, wires dependencies.
///
/// If `log_profile_override` is `Some`, the `UC_LOG_PROFILE` env var is set
/// before tracing initialization so the subscriber picks up the desired profile.
fn build_core(
    pairing_runtime_owner: PairingRuntimeOwner,
    log_profile_override: Option<uc_observability::LogProfile>,
) -> anyhow::Result<(AppConfig, crate::assembly::WiredDependencies)> {
    // Apply log profile override before tracing init
    if let Some(profile) = log_profile_override {
        std::env::set_var("UC_LOG_PROFILE", profile.to_string());
    }

    // Idempotent -- safe to call multiple times
    crate::tracing::init_tracing_subscriber()?;

    let config = AppConfig::empty();

    let wired = wire_dependencies(&config, pairing_runtime_owner)
        .map_err(|e| anyhow::anyhow!("Dependency wiring failed: {}", e))?;

    Ok((config, wired))
}

fn gui_pairing_runtime_owner() -> PairingRuntimeOwner {
    PairingRuntimeOwner::ExternalDaemon
}

fn cli_pairing_runtime_owner() -> PairingRuntimeOwner {
    PairingRuntimeOwner::ExternalDaemon
}

fn daemon_pairing_runtime_owner() -> PairingRuntimeOwner {
    PairingRuntimeOwner::CurrentProcess
}

/// Build GUI bootstrap context. Returns raw AppDeps (not CoreRuntime) so that
/// AppRuntime::with_setup() in uc-tauri can construct CoreRuntime with the
/// correct emitter cell, lifecycle status, and task registry.
///
/// Slice 4 P5a-4: 旧 libp2p `PairingFacade` 不再在 GUI 进程构造,GUI
/// 通过 daemon HTTP setup-v2 流程驱动 pairing,本函数只负责 deps + 路径。
pub fn build_gui_app() -> anyhow::Result<GuiBootstrapContext> {
    let (config, wired) = build_core(gui_pairing_runtime_owner(), None)?;

    let deps = wired.deps;
    let background = wired.background;
    let storage_paths = get_storage_paths(&config)?;

    // [Codex Review R1] Return AppDeps, NOT CoreRuntime.
    // CoreRuntime is constructed by AppRuntime::with_setup() in uc-tauri,
    // which needs to create the shared emitter cell, task registry, etc.
    Ok(GuiBootstrapContext {
        deps,
        background,
        storage_paths,
        config,
    })
}

/// Build CLI bootstrap context. Returns AppDeps for the caller to construct
/// CoreRuntime as needed. No background workers are started.
pub fn build_cli_context() -> anyhow::Result<CliBootstrapContext> {
    build_cli_context_with_profile(Some(uc_observability::LogProfile::Cli))
}

/// Build CLI bootstrap context with an explicit log profile override.
///
/// When `verbose` mode is active, callers pass `Some(LogProfile::Dev)` to
/// get full console tracing. The default `build_cli_context()` uses `Cli`
/// profile which suppresses console output.
pub fn build_cli_context_with_profile(
    log_profile: Option<uc_observability::LogProfile>,
) -> anyhow::Result<CliBootstrapContext> {
    let (config, wired) = build_core(cli_pairing_runtime_owner(), log_profile)?;

    // [Codex Review R1] Return AppDeps, not CoreRuntime.
    // CLI entry point constructs CoreRuntime itself with appropriate emitter.
    Ok(CliBootstrapContext {
        deps: wired.deps,
        config,
    })
}

/// Slice 1 CLI composition-root entry. Returns the full
/// [`crate::assembly::WiredDependencies`] so the caller can hand it to
/// [`crate::space_setup::build_space_setup_assembly`]; unlike
/// [`build_cli_context_with_profile`], this does not flatten to `AppDeps`
/// and therefore preserves access to `trusted_peer_repo` and other Slice
/// 1-only ports the `SpaceSetupFacade` needs.
pub fn build_slice1_cli_context(
    log_profile: Option<uc_observability::LogProfile>,
) -> anyhow::Result<(AppConfig, crate::assembly::WiredDependencies)> {
    build_core(cli_pairing_runtime_owner(), log_profile)
}

/// Build daemon bootstrap context. Returns AppDeps + background deps.
/// Caller constructs CoreRuntime and starts background workers.
///
/// Slice 2 Phase 3 · T5 — also binds the iroh node + builds the full
/// `SpaceSetupAssembly` (Slice 1 pairing + Slice 2 presence + clipboard
/// handlers) and exposes `clipboard_sync_facade` so daemon workers can
/// dispatch / subscribe via the iroh stack instead of the deprecated
/// libp2p `ClipboardOutboundTransportPort` / `ClipboardInboundTransportPort`.
///
/// Slice 4 P5a-4: 旧 libp2p `PairingFacade` 已下线,daemon 不再构造它,
/// 也不再向 ctx 暴露 trusted_peer_repo / key_slot_store(消费者
/// `DaemonPairingHost` 已删)。trusted_peer_repo 仍由 `wired` 透传给
/// `build_space_setup_assembly` — 那里是 setup-v2 流程的合法消费方。
pub async fn build_daemon_app() -> anyhow::Result<DaemonBootstrapContext> {
    let (config, wired) = build_core(daemon_pairing_runtime_owner(), None)?;
    let storage_paths = get_storage_paths(&config)?;

    // Build the iroh-stack assembly on the caller's runtime. Must NOT spin up
    // a throwaway current-thread rt here: `Endpoint::bind` spawns magicsock /
    // relay / STUN actors via `tokio::spawn`, which attach to whatever runtime
    // is running the bind. If that runtime drops (as a short-lived local rt
    // would), those actors are aborted and the Endpoint becomes a zombie —
    // `connect()` then returns "Unable to connect to remote" instantly and
    // `accept` sees no incoming traffic. Keeping the bind on the caller's
    // long-lived daemon runtime keeps iroh's tasks alive for the process
    // lifetime.
    let space_setup_assembly = build_space_setup_assembly(&wired, IrohNodeConfig::default())
        .await
        .map_err(|e| anyhow::anyhow!("Slice 1+ assembly build failed: {e}"))?;

    // Now safe to consume `wired` — assembly is built and owns its own
    // Arcs to the underlying ports.
    let deps = wired.deps;
    let background = wired.background;
    let emitter_cell = wired.emitter_cell;

    // Phase A.2: inject AdmitMemberUseCase so joiner-side `Granted` also
    // registers the sponsor peer as a local space member. Failure to admit
    // only logs WARN and does not block `Granted` itself.
    let admit_member = Arc::new(AdmitMemberUseCase::new(deps.device.member_repo.clone()));
    let space_access_facade = Arc::new(SpaceAccessFacade::with_admit_member(admit_member));

    // Same Arc the assembly holds — handed up to ctx so daemon entrypoint
    // (T6) can wire it into the two clipboard workers without unpacking
    // the assembly.
    let clipboard_sync_facade = Arc::clone(&space_setup_assembly.clipboard_sync);

    Ok(DaemonBootstrapContext {
        deps,
        background,
        emitter_cell,
        space_access_facade,
        storage_paths,
        config,
        clipboard_sync_facade,
        space_setup_assembly,
    })
}
