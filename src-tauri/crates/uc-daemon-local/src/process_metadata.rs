use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};

use uc_app::app_paths::AppPaths;
use uc_platform::app_dirs::DirsAppDirsAdapter;
use uc_platform::ports::AppDirsPort;

/// Provides the process-wide singleton `DaemonPidManager` used by standalone helpers.
///
/// On first call this initializes the manager by resolving application directories; any
/// initialization failure is returned as an error.
///
/// # Returns
///
/// A reference to the initialized `DaemonPidManager`.
///
/// # Examples
///
/// ```
/// let mgr = default_manager().expect("failed to initialize default daemon PID manager");
/// let pid_path = mgr.pid_path();
/// ```
fn default_manager() -> Result<&'static DaemonPidManager> {
    static DEFAULT_MANAGER: OnceLock<Result<DaemonPidManager, String>> = OnceLock::new();
    DEFAULT_MANAGER
        .get_or_init(|| {
            let adapter = DirsAppDirsAdapter::new();
            adapter
                .get_app_dirs()
                .context("failed to resolve application directories")
                .map(|app_dirs| DaemonPidManager::new(AppPaths::from_app_dirs(&app_dirs)))
                .map_err(|e| format!("{e:#}"))
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Manages the daemon PID metadata file lifecycle.
#[derive(Debug, Clone)]
pub struct DaemonPidManager {
    app_paths: AppPaths,
}

impl DaemonPidManager {
    /// Creates a new DaemonPidManager from the provided `AppPaths`.
    ///
    /// # Examples
    ///
    /// ```
    /// let app_paths = AppPaths::default();
    /// let mgr = DaemonPidManager::new(app_paths);
    /// // use `mgr` to read/write the daemon PID file
    /// ```
    pub fn new(app_paths: AppPaths) -> Self {
        Self { app_paths }
    }

    /// Returns the filesystem path where the daemon PID file for the current app/profile is stored.
    ///
    /// This is derived from the manager's configured `AppPaths`.
    fn pid_path(&self) -> PathBuf {
        self.app_paths.daemon_pid_path()
    }

    /// Write the current process PID to the manager's PID file.
    ///
    /// Creates the PID file's parent directory if needed, writes the current
    /// process ID as a decimal string, and repairs file permissions on Unix.
    ///
    /// # Examples
    ///
    /// ```
    /// // Given a `DaemonPidManager` instance `mgr`:
    /// let pid = mgr.write_current_pid().unwrap();
    /// assert_eq!(pid, std::process::id());
    /// ```
    ///
    /// # Returns
    ///
    /// `Ok(pid)` containing the written PID on success, `Err` with context on failure.
    pub fn write_current_pid(&self) -> Result<u32> {
        let pid_path = self.pid_path();
        let pid = std::process::id();

        if let Some(parent) = pid_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create daemon pid directory {}", parent.display())
            })?;
        }

        fs::write(&pid_path, pid.to_string())
            .with_context(|| format!("failed to write daemon pid file {}", pid_path.display()))?;

        repair_pid_permissions(&pid_path)?;
        Ok(pid)
    }

    /// Removes the daemon PID metadata file for this manager's configured path.
    ///
    /// If the PID file is missing, this operation succeeds and returns `Ok(())`.
    /// Any other I/O error is returned with context that includes the PID file path.
    ///
    /// # Examples
    ///
    /// ```
    /// let mgr = /* obtain a DaemonPidManager configured for your environment */;
    /// mgr.remove_pid_file().unwrap();
    /// ```
    pub fn remove_pid_file(&self) -> Result<()> {
        let pid_path = self.pid_path();
        match fs::remove_file(&pid_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(anyhow::Error::new(error).context(format!(
                "failed to remove daemon pid file {}",
                pid_path.display()
            ))),
        }
    }

    /// Returns the daemon PID stored in the manager's PID file, if present.
    ///
    /// Reads the PID file at the manager's resolved path, trims whitespace, and parses its contents as a `u32`.
    ///
    /// # Returns
    ///
    /// `Some(pid)` if the PID file exists and contains a valid `u32`, `None` if the PID file does not exist.
    pub fn read_pid_file(&self) -> Result<Option<u32>> {
        let pid_path = self.pid_path();
        if !pid_path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&pid_path)
            .with_context(|| format!("failed to read daemon pid file {}", pid_path.display()))?;
        let pid = raw.trim().parse::<u32>().with_context(|| {
            format!(
                "failed to parse daemon pid file {} contents as u32",
                pid_path.display()
            )
        })?;
        Ok(Some(pid))
    }

    /// Resolve the daemon PID file path used by this manager for tests.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// // `mgr` is a `DaemonPidManager`.
    /// let path = mgr.pid_path_for_testing();
    /// println!("{}", path.display());
    /// ```
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn pid_path_for_testing(&self) -> PathBuf {
        self.pid_path()
    }
}

/// Ensures the daemon PID file is readable/writable only by the owner (mode 0o600) on Unix; does nothing on non-Unix platforms.
///
/// On Unix, this updates the file mode to `0o600` when the current mode differs. On non-Unix platforms the function is a no-op.
///
/// # Errors
///
/// Returns an error with context if reading metadata or setting permissions fails on Unix.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// let path = Path::new("/tmp/.daemon-pid");
/// // Ignore the result in examples; real code should handle the error.
/// let _ = crate::process_metadata::repair_pid_permissions(path);
/// ```
fn repair_pid_permissions(pid_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(pid_path).with_context(|| {
            format!("failed to read daemon pid metadata {}", pid_path.display())
        })?;
        let current_mode = metadata.permissions().mode() & 0o777;
        if current_mode != 0o600 {
            fs::set_permissions(pid_path, fs::Permissions::from_mode(0o600)).with_context(
                || {
                    format!(
                        "failed to repair daemon pid permissions {}",
                        pid_path.display()
                    )
                },
            )?;
        }
    }

    Ok(())
}

// Backward-compatible standalone functions for external callers.

/// Returns the daemon PID for the current application profile, if one is stored.
///
/// # Examples
///
/// ```
/// // Returns `Ok(Some(pid))` when a PID file exists, `Ok(None)` when it does not.
/// let pid_opt = read_pid_file().unwrap();
/// match pid_opt {
///     Some(pid) => println!("Daemon PID: {}", pid),
///     None => println!("No daemon PID stored"),
/// }
/// ```
pub fn read_pid_file() -> Result<Option<u32>> {
    default_manager()?.read_pid_file()
}

/// Write the current process PID to the configured daemon PID file.
///
/// Returns the PID that was written.
///
/// # Examples
///
/// ```no_run
/// let pid = write_current_pid().unwrap();
/// assert_eq!(pid, std::process::id());
/// ```
pub fn write_current_pid() -> Result<u32> {
    default_manager()?.write_current_pid()
}

/// Removes the daemon PID metadata file for the current application profile.
///
/// If the PID file does not exist, this function returns `Ok(())`. On other failures it returns
/// an error with context describing the failed removal.
///
/// # Returns
///
/// `()` on success, or an `anyhow::Error` on failure.
///
/// # Examples
///
/// ```
/// // Remove the pid file for the current profile; succeeds even if no file was present.
/// uc_daemon_local::process_metadata::remove_pid_file().unwrap();
/// ```
pub fn remove_pid_file() -> Result<()> {
    default_manager()?.remove_pid_file()
}

/// Compute the filesystem path where the daemon PID metadata file for the current application profile is stored.
///
/// # Returns
///
/// The resolved PID file path as a `PathBuf`.
///
/// # Examples
///
/// ```
/// let path = uc_daemon_local::process_metadata::resolve_pid_path().unwrap();
/// assert!(path.ends_with(".daemon-pid"));
/// ```
pub fn resolve_pid_path() -> Result<PathBuf> {
    Ok(default_manager()?.pid_path().to_path_buf())
}
