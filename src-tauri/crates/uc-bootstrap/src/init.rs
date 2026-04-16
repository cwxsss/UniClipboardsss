//! Bootstrap initialization functions
//!
//! This module contains initialization functions that run during application startup.

use std::sync::Arc;
use uc_core::ports::SettingsPort;

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
