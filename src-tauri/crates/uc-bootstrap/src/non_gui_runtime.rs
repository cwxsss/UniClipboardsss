//! # Non-GUI Runtime Helpers
//!
//! Provides [`LoggingHostEventEmitter`] and [`build_non_gui_runtime()`] for
//! constructing a [`CoreRuntime`] in non-GUI entry points (daemon, CLI).
//!
//! [`LoggingHostEventEmitter`] logs event type names via `tracing::debug!`
//! without printing inner payloads (which may contain sensitive data like
//! clipboard content, pairing codes, or file paths).

use std::sync::Arc;

use uc_app::runtime::CoreRuntime;
use uc_app::task_registry::TaskRegistry;
use uc_app::AppDeps;
use uc_application::facade::{
    AppPaths, EmitError, HostEvent, HostEventEmitterPort, InMemoryLifecycleStatus,
};
use uc_core::clipboard::ClipboardIntegrationMode;

// ---------------------------------------------------------------------------
// LoggingHostEventEmitter
// ---------------------------------------------------------------------------

/// Event emitter that logs event type names via `tracing::debug!`.
///
/// Always returns `Ok(())` — infallible by design. Inner event payloads are
/// NOT logged because they may contain sensitive data (clipboard content,
/// pairing codes/fingerprints, transfer file paths).
pub struct LoggingHostEventEmitter;

impl HostEventEmitterPort for LoggingHostEventEmitter {
    fn emit(&self, event: HostEvent) -> Result<(), EmitError> {
        match event {
            HostEvent::Clipboard(_) => {
                tracing::debug!(event_type = "clipboard", "host event (non-gui)");
            }
            HostEvent::Transfer(_) => {
                tracing::debug!(event_type = "transfer", "host event (non-gui)");
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// build_non_gui_runtime
// ---------------------------------------------------------------------------

/// Construct a [`CoreRuntime`] for non-GUI entry points (daemon, CLI).
///
/// Uses [`LoggingHostEventEmitter`] as the permanent emitter (no swap needed
/// in non-GUI modes), `InMemoryLifecycleStatus`, and the
/// `UC_CLIPBOARD_MODE` environment override.
///
/// # Arguments
///
/// * `deps` — Pre-wired application dependencies from `wire_dependencies()`.
/// * `storage_paths` — Resolved storage paths (caller resolves via
///   `get_storage_paths(&config)` before calling this function).
pub fn build_non_gui_runtime(
    deps: AppDeps,
    storage_paths: AppPaths,
) -> anyhow::Result<CoreRuntime> {
    let emitter: Arc<dyn HostEventEmitterPort> = Arc::new(LoggingHostEventEmitter);
    let emitter_cell = Arc::new(std::sync::RwLock::new(emitter));
    build_non_gui_runtime_with_emitter(deps, storage_paths, emitter_cell)
}

/// Construct a [`CoreRuntime`] for non-GUI entry points with an explicit
/// shared emitter cell. Daemon uses this so its `DaemonApiEventEmitter`
/// can be swapped in after construction.
pub fn build_non_gui_runtime_with_emitter(
    deps: AppDeps,
    storage_paths: AppPaths,
    emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>,
) -> anyhow::Result<CoreRuntime> {
    let lifecycle_status = Arc::new(InMemoryLifecycleStatus::new());
    let task_registry = Arc::new(TaskRegistry::new());
    let clipboard_integration_mode = resolve_clipboard_integration_mode();

    Ok(CoreRuntime::new(
        deps,
        emitter_cell,
        lifecycle_status,
        clipboard_integration_mode,
        task_registry,
        storage_paths,
    ))
}

// ---------------------------------------------------------------------------
// build_cli_runtime
// ---------------------------------------------------------------------------

/// Construct a [`CoreRuntime`] for CLI entry points with a single function call.
///
/// This helper combines the common 4-step bootstrap sequence used by CLI commands:
/// 1. Build CLI context via `build_cli_context_with_profile()`
/// 2. Get storage paths via `get_storage_paths()`
/// 3. Build non-GUI runtime via `build_non_gui_runtime()`
///
/// Callers then create `CoreUseCases::new(&runtime)` to access use cases.
///
/// # Arguments
///
/// * `log_profile` — Log profile override (e.g., `Some(LogProfile::Cli)` or `Some(LogProfile::Dev)`)
pub fn build_cli_runtime(
    log_profile: Option<uc_observability::LogProfile>,
) -> anyhow::Result<CoreRuntime> {
    let ctx = crate::builders::build_cli_context_with_profile(log_profile)?;
    let storage_paths = crate::assembly::get_storage_paths(&ctx.config)?;
    let runtime = build_non_gui_runtime(ctx.deps, storage_paths)?;
    Ok(runtime)
}

/// Parse a raw string into a [`ClipboardIntegrationMode`].
///
/// Returns `Full` when `raw` is `None`, empty, or an unrecognized value.
/// Returns `Passive` only when the value is `"passive"` (case-insensitive).
pub fn parse_clipboard_integration_mode(raw: Option<&str>) -> ClipboardIntegrationMode {
    let Some(raw_value) = raw else {
        return ClipboardIntegrationMode::Full;
    };

    let normalized = raw_value.trim();
    if normalized.eq_ignore_ascii_case("passive") {
        return ClipboardIntegrationMode::Passive;
    }
    if normalized.eq_ignore_ascii_case("full") {
        return ClipboardIntegrationMode::Full;
    }

    tracing::warn!(
        uc_clipboard_mode = %raw_value,
        "Invalid UC_CLIPBOARD_MODE value; falling back to full integration"
    );
    ClipboardIntegrationMode::Full
}

/// Resolve the clipboard integration mode from the `UC_CLIPBOARD_MODE` env var.
///
/// Defaults to [`ClipboardIntegrationMode::Full`] when the variable is unset.
/// Used by both GUI and non-GUI runtimes to determine clipboard behavior.
pub fn resolve_clipboard_integration_mode() -> ClipboardIntegrationMode {
    let raw = std::env::var("UC_CLIPBOARD_MODE").ok();
    parse_clipboard_integration_mode(raw.as_deref())
}
