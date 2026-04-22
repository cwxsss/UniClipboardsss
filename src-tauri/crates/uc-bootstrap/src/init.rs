//! Bootstrap initialization functions
//!
//! This module contains initialization functions that run during application startup.

use std::sync::Arc;
use uc_app::app_paths::AppPaths;
use uc_application::setup::IsSetupComplete;
use uc_core::config::AppConfig;
use uc_core::ports::{SettingsPort, SetupStatusPort};
use uc_infra::FileSetupStatusRepository;

use crate::assembly::{get_default_app_dirs, get_storage_paths};

/// Returns `true` when the active profile has a completed Space setup.
///
/// Composition-root helper (§3 layer rule: only bootstrap may mix infra
/// adapters + application use cases). Wires
/// [`FileSetupStatusRepository`] into the application-layer
/// [`IsSetupComplete`] use case so the domain question ("has setup
/// completed?") is answered by a use case, not by bootstrap peeking at
/// file formats.
///
/// Profile-aware: goes through `get_storage_paths` which applies
/// `apply_profile_suffix` against `UC_PROFILE` env var (set by
/// `uniclipboard-cli`'s `main.rs` from `--profile`), so the same vault
/// dir that `init` / `join` wrote into is the one we read back.
///
/// Legacy fallback: the libp2p-era Tauri `uniclipboard setup` command
/// wrote a `.initialized_encryption` marker file. The domain use case
/// doesn't know about that (Slice 5 will delete the path entirely), so
/// the back-compat check stays here in the composition root as a
/// pragmatic fallback — remove it once nobody has pre-Slice 1 state
/// left.
pub async fn is_setup_complete() -> anyhow::Result<bool> {
    let paths = get_storage_paths(&AppConfig::empty())
        .map_err(|e| anyhow::anyhow!("resolve storage paths: {e}"))?;

    let setup_status: Arc<dyn SetupStatusPort> = Arc::new(
        FileSetupStatusRepository::with_defaults(paths.vault_dir.clone()),
    );
    let use_case = IsSetupComplete::from_ports(setup_status);
    if use_case.execute().await.unwrap_or(false) {
        return Ok(true);
    }

    // Legacy back-compat only (§6.4 rationale). The `IsSetupComplete`
    // use case is the authoritative Slice 1+ answer.
    let legacy_marker = AppPaths::from_app_dirs(&get_default_app_dirs()?).encryption_marker_path();
    Ok(legacy_marker.exists())
}

/// Ensures the device has a valid name by initializing it with the system hostname if empty.
///
/// When the application starts, this function checks if `device_name` is `None` or an empty
/// string. If so, it fetches the system hostname and saves it as the default device name.
///
/// # Arguments
///
/// * `settings` - A reference to the settings port implementation
///
/// # Returns
///
/// * `Result<(), Box<dyn std::error::Error>>` - Ok on success, error on failure
///
/// # Behavior
///
/// - If `device_name` is `None` or empty, fetches system hostname and saves it
/// - If `device_name` already has a value, does nothing
/// - Logs the initialization event when setting hostname
///
/// # Example
///
/// ```no_run
/// use uc_bootstrap::ensure_default_device_name;
/// use uc_core::ports::SettingsPort;
/// use std::sync::Arc;
///
/// # async fn example(settings: Arc<dyn SettingsPort>) -> Result<(), Box<dyn std::error::Error>> {
/// ensure_default_device_name(settings).await?;
/// # Ok(())
/// # }
/// ```
pub async fn ensure_default_device_name(
    settings: Arc<dyn SettingsPort>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut current_settings = settings.load().await?;

    // Check if device_name is None or empty string
    let needs_initialization = current_settings.general.device_name.is_none()
        || current_settings.general.device_name.as_deref() == Some("");

    if needs_initialization {
        let hostname = gethostname::gethostname()
            .to_str()
            .unwrap_or("Uniclipboard Device")
            .to_string();

        tracing::info!("Initializing default device name: {}", hostname);

        current_settings.general.device_name = Some(hostname);
        settings.save(&current_settings).await?;
    }

    Ok(())
}
