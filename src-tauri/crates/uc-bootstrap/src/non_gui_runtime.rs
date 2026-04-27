//! # Non-GUI Runtime Helpers
//!
//! Provides [`LoggingHostEventEmitter`] and [`build_non_gui_bundle()`] for
//! non-GUI entry points (daemon, CLI). D16-2 retired the legacy `CoreRuntime`
//! wrapper; helpers here now return a flat [`NonGuiBundle`] that the caller
//! destructures into independent locals.
//!
//! [`LoggingHostEventEmitter`] logs event type names via `tracing::debug!`
//! without printing inner payloads (which may contain sensitive data like
//! clipboard content, pairing codes, or file paths).

use std::sync::Arc;

use uc_application::deps::AppDeps;
use uc_application::facade::{
    AppFacade, AppFacadeParts, AppPaths, ClipboardHistoryFacade, ClipboardHistoryFacadeDeps,
    DeviceFacade, EmitError, EncryptionFacade, EncryptionFacadeDeps, HostEvent,
    HostEventEmitterPort, InMemoryLifecycleStatus, LifecycleFacade, LifecycleFacadeDeps,
    LifecycleStatusGateway, ResourceFacade, ResourceFacadeDeps, SearchFacade, SearchFacadeDeps,
    SettingsFacade, StorageFacade, StorageFacadeDeps,
};
use uc_core::clipboard::ClipboardIntegrationMode;

use crate::task_registry::TaskRegistry;

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
// NonGuiBundle
// ---------------------------------------------------------------------------

/// Flat bundle of bootstrap-built handles consumed by daemon entry points.
///
/// Replaces the retired `CoreRuntime` wrapper. Composition-root code
/// destructures the bundle into independent locals (`deps`, `task_registry`,
/// `lifecycle_status`, etc.) and feeds them into facade construction.
pub struct NonGuiBundle {
    pub deps: AppDeps,
    pub storage_paths: AppPaths,
    pub emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>,
    pub lifecycle_status: Arc<dyn LifecycleStatusGateway>,
    pub task_registry: Arc<TaskRegistry>,
    pub clipboard_integration_mode: ClipboardIntegrationMode,
}

/// Construct a [`NonGuiBundle`] for non-GUI entry points with an explicit
/// shared emitter cell. Daemon uses this so its `DaemonApiEventEmitter`
/// can be swapped in after construction.
pub fn build_non_gui_bundle(
    deps: AppDeps,
    storage_paths: AppPaths,
    emitter_cell: Arc<std::sync::RwLock<Arc<dyn HostEventEmitterPort>>>,
) -> anyhow::Result<NonGuiBundle> {
    let lifecycle_status: Arc<dyn LifecycleStatusGateway> =
        Arc::new(InMemoryLifecycleStatus::new());
    let task_registry = Arc::new(TaskRegistry::new());
    let clipboard_integration_mode = resolve_clipboard_integration_mode();

    Ok(NonGuiBundle {
        deps,
        storage_paths,
        emitter_cell,
        lifecycle_status,
        task_registry,
        clipboard_integration_mode,
    })
}

/// Construct an [`AppFacade`] for CLI entry points.
///
/// CLI commands (`setup`, `space_status`) need a stable application-layer
/// entry point per `uc-application/AGENTS.md` §11.4. This helper assembles
/// the facade subset CLI cares about (encryption / settings / device /
/// clipboard_history / search / lifecycle / storage / resource) and leaves
/// the daemon-only fields (`space_setup`, `member_roster`, `clipboard_restore`)
/// as `None`.
///
/// # Arguments
///
/// * `log_profile` — Log profile override (e.g., `Some(LogProfile::Cli)`).
pub fn build_cli_app_facade(
    log_profile: Option<uc_observability::LogProfile>,
) -> anyhow::Result<Arc<AppFacade>> {
    let ctx = crate::builders::build_cli_context_with_profile(log_profile)?;
    let storage_paths = crate::assembly::get_storage_paths(&ctx.config)?;
    let deps = ctx.deps;
    let lifecycle_status: Arc<dyn LifecycleStatusGateway> =
        Arc::new(InMemoryLifecycleStatus::new());

    Ok(Arc::new(AppFacade::new(AppFacadeParts {
        space_setup: None,
        member_roster: None,
        lifecycle: Arc::new(LifecycleFacade::new(LifecycleFacadeDeps {
            status: lifecycle_status,
        })),
        encryption: Arc::new(EncryptionFacade::new(EncryptionFacadeDeps {
            setup_status: deps.setup_status.clone(),
            space_access: deps.security.space_access.clone(),
        })),
        resource: Arc::new(ResourceFacade::new(ResourceFacadeDeps {
            representation_repo: deps.clipboard.representation_repo.clone(),
            thumbnail_repo: deps.storage.thumbnail_repo.clone(),
            blob_store: deps.storage.blob_store.clone(),
        })),
        clipboard_history: Arc::new(ClipboardHistoryFacade::new(ClipboardHistoryFacadeDeps {
            entry_repo: deps.clipboard.clipboard_entry_repo.clone(),
            selection_repo: deps.clipboard.selection_repo.clone(),
            representation_repo: deps.clipboard.representation_repo.clone(),
            event_writer: deps.clipboard.clipboard_event_repo.clone(),
            payload_resolver: deps.clipboard.payload_resolver.clone(),
            blob_store: deps.storage.blob_store.clone(),
            thumbnail_repo: deps.storage.thumbnail_repo.clone(),
            file_transfer_repo: deps.storage.file_transfer_repo.clone(),
            search_index: Some(deps.search.search_index.clone()),
            file_cache_dir: Some(storage_paths.cache_dir.clone()),
        })),
        clipboard_restore: None,
        search: Arc::new(SearchFacade::new(SearchFacadeDeps {
            search_index: deps.search.search_index.clone(),
            coordinator: None,
        })),
        settings: Arc::new(SettingsFacade::new(deps.settings.clone())),
        device: Arc::new(DeviceFacade::new(
            deps.device.device_identity.clone(),
            deps.settings.clone(),
        )),
        storage: Arc::new(StorageFacade::new(StorageFacadeDeps {
            db_path: storage_paths.db_path.clone(),
            vault_dir: storage_paths.vault_dir.clone(),
            cache_dir: storage_paths.cache_dir.clone(),
            logs_dir: storage_paths.logs_dir.clone(),
            app_data_root_dir: storage_paths.app_data_root_dir.clone(),
            cache_fs: deps.system.cache_fs.clone(),
        })),
    })))
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
