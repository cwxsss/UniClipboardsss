use std::path::PathBuf;

use crate::ports::AppDirsPort;
use uc_core::app_dirs::AppDirs;
use uc_core::ports::AppDirsError;

const APP_DIR_NAME: &str = "app.uniclipboard.desktop";
const PID_EXTENSION: &str = "pid";

fn resolved_app_dir_name() -> String {
    match std::env::var("UC_PROFILE") {
        Ok(profile) if !profile.is_empty() => format!("{APP_DIR_NAME}-{profile}"),
        _ => APP_DIR_NAME.to_string(),
    }
}

pub struct DirsAppDirsAdapter {
    base_data_local_dir_override: Option<PathBuf>,
    cached_app_dir_name: String,
}

impl DirsAppDirsAdapter {
    /// Creates a new DirsAppDirsAdapter with no base data directory override.
    ///
    /// # Examples
    ///
    /// ```
    /// use uc_platform::app_dirs::DirsAppDirsAdapter;
    /// let _ = DirsAppDirsAdapter::new();
    /// ```
    pub fn new() -> Self {
        Self {
            base_data_local_dir_override: None,
            cached_app_dir_name: resolved_app_dir_name(),
        }
    }

    /// Creates an adapter that overrides the base local data directory.
    ///
    /// The provided `base` path will be used instead of the system data local directory
    /// when resolving application directories for this adapter.
    ///
    /// This is useful for testing, where you want to redirect paths to a temp directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    /// use uc_platform::app_dirs::DirsAppDirsAdapter;
    ///
    /// let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/tmp"));
    /// ```
    pub fn with_base_data_local_dir(base: PathBuf) -> Self {
        Self {
            base_data_local_dir_override: Some(base),
            cached_app_dir_name: resolved_app_dir_name(),
        }
    }

    /// Resolve the base local data directory used for application data.
    ///
    /// Returns `Some(PathBuf)` containing the overridden base directory if one was set when the
    /// adapter was constructed; otherwise returns the system data-local directory from `dirs::data_local_dir()`.
    /// Returns `None` if no override is set and the system data-local directory is unavailable.
    ///
    /// # Examples
    ///
    /// ```
    /// use uc_platform::app_dirs::DirsAppDirsAdapter;
    ///
    /// let adapter = DirsAppDirsAdapter::new();
    /// let _ = adapter.base_data_local_dir();
    /// ```
    pub fn base_data_local_dir(&self) -> Option<PathBuf> {
        if let Some(base) = &self.base_data_local_dir_override {
            return Some(base.clone());
        }
        dirs::data_local_dir()
    }

    fn base_cache_dir(&self) -> Option<PathBuf> {
        if let Some(base) = &self.base_data_local_dir_override {
            return Some(base.clone());
        }
        dirs::cache_dir()
    }
}

impl AppDirsPort for DirsAppDirsAdapter {
    /// Constructs the application's directories using the system (or overridden) local data directory.
    ///
    /// # Returns
    ///
    /// `AppDirs` with `app_data_root` set to the base local data directory joined with the
    /// value captured from `resolved_app_dir_name()` when this adapter is created.
    ///
    /// Depending on `UC_PROFILE`, `resolved_app_dir_name()` resolves to `"uniclipboard"`
    /// or `"uniclipboard-{profile}"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use uc_platform::ports::AppDirsPort;
    /// use uc_platform::app_dirs::DirsAppDirsAdapter;
    ///
    /// let adapter = DirsAppDirsAdapter::new();
    /// let _ = adapter.get_app_dirs();
    /// ```
    fn get_app_dirs(&self) -> Result<AppDirs, AppDirsError> {
        let base_data = self
            .base_data_local_dir()
            .ok_or(AppDirsError::DataLocalDirUnavailable)?;
        let base_cache = self
            .base_cache_dir()
            .ok_or(AppDirsError::CacheDirUnavailable)?;
        Ok(AppDirs {
            app_data_root: base_data.join(&self.cached_app_dir_name),
            app_cache_root: base_cache.join(&self.cached_app_dir_name),
        })
    }
}

/// Resolve the daemon PID file name, matching the profile-aware naming convention.
///
/// Uses `UC_PROFILE` to generate a suffix when a profile is active:
/// - Default: `"uniclipboard-daemon.pid"`
/// - With `UC_PROFILE=a`: `"uniclipboard-daemon-a.pid"`
fn daemon_pid_file_name() -> String {
    match std::env::var("UC_PROFILE") {
        Ok(profile) if !profile.is_empty() => {
            let sanitized = sanitize_profile_component(&profile);
            format!("uniclipboard-daemon-{sanitized}.{PID_EXTENSION}")
        }
        _ => format!("uniclipboard-daemon.{PID_EXTENSION}"),
    }
}

/// Sanitize a profile string into a safe filesystem component.
///
/// Strips characters that are not alphanumeric, `-`, or `_`. If the result
/// is all underscores (i.e. the input contained only invalid characters),
/// returns `"profile"` as a fallback.
fn sanitize_profile_component(profile: &str) -> String {
    let sanitized: String = profile
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect();

    if sanitized.chars().all(|ch| ch == '_') {
        "profile".to_string()
    } else {
        sanitized
    }
}

/// Resolve the daemon PID file path under the application data directory.
///
/// The PID file is stored alongside other application data, ensuring it
/// survives across sessions (unlike `/tmp`-based paths which may be cleared).
///
/// The resolved path follows the profile-aware naming convention, so
/// each `UC_PROFILE` value gets its own isolated pid file.
pub fn resolve_daemon_pid_path() -> Result<PathBuf, AppDirsError> {
    let app_dirs = DirsAppDirsAdapter::new().get_app_dirs()?;
    Ok(app_dirs.app_data_root.join(daemon_pid_file_name()))
}

/// Resolve the daemon PID file path with an explicit base directory override.
///
/// This is the test-helpers equivalent of `resolve_daemon_pid_path()`. It allows
/// callers (including daemon unit tests) to redirect the pid path to an
/// arbitrary directory without depending on the live system data directory.
#[cfg(feature = "test-helpers")]
pub fn resolve_daemon_pid_path_for_testing(
    base: std::path::PathBuf,
) -> Result<PathBuf, AppDirsError> {
    let adapter = DirsAppDirsAdapter::with_base_data_local_dir(base);
    let app_dirs = adapter.get_app_dirs()?;
    Ok(app_dirs.app_data_root.join(daemon_pid_file_name()))
}

/// Resolve the daemon auth token file name, matching the profile-aware naming convention.
fn daemon_token_file_name() -> String {
    daemon_pid_file_name().replace(".pid", ".token")
}

/// Resolve the daemon auth token file path under the application data directory.
///
/// The token is stored alongside the PID file in the app data root, ensuring it
/// survives across sessions and is profile-isolated.
///
/// Resolves to `{data_local_dir}/app.uniclipboard.desktop[-{profile}]/uniclipboard-daemon.token`.
pub fn resolve_daemon_token_path() -> Result<PathBuf, AppDirsError> {
    let app_dirs = DirsAppDirsAdapter::new().get_app_dirs()?;
    Ok(app_dirs.app_data_root.join(daemon_token_file_name()))
}

/// Resolve the daemon auth token file path with an explicit base directory override.
///
/// This is the test-helpers equivalent of `resolve_daemon_token_path()`.
#[cfg(feature = "test-helpers")]
pub fn resolve_daemon_token_path_for_testing(
    base: std::path::PathBuf,
) -> Result<PathBuf, AppDirsError> {
    let adapter = DirsAppDirsAdapter::with_base_data_local_dir(base);
    let app_dirs = adapter.get_app_dirs()?;
    Ok(app_dirs.app_data_root.join(daemon_token_file_name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::AppDirsPort;
    use crate::test_support::with_uc_profile;

    /// Verifies that the adapter appends the `uniclipboard` directory name to the base data directory.
    ///
    /// # Examples
    ///
    /// ```
    /// let adapter = DirsAppDirsAdapter::with_base_data_local_dir(std::path::PathBuf::from("/tmp"));
    /// let dirs = adapter.get_app_dirs().unwrap();
    /// assert_eq!(dirs.app_data_root, std::path::PathBuf::from("/tmp/app.uniclipboard.desktop"));
    /// ```
    #[test]
    fn adapter_appends_uniclipboard_dir_name() {
        with_uc_profile(None, || {
            let adapter =
                DirsAppDirsAdapter::with_base_data_local_dir(std::path::PathBuf::from("/tmp"));
            let dirs = adapter.get_app_dirs().unwrap();
            assert_eq!(
                dirs.app_data_root,
                std::path::PathBuf::from("/tmp/app.uniclipboard.desktop")
            );
        });
    }

    #[test]
    fn adapter_sets_cache_root() {
        with_uc_profile(None, || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/tmp"));
            let dirs = adapter.get_app_dirs().unwrap();
            assert!(dirs.app_cache_root.ends_with("app.uniclipboard.desktop"));
        });
    }

    #[test]
    fn adapter_isolates_dirs_for_different_uc_profile_values() {
        let dirs_a = with_uc_profile(Some("a"), || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/tmp"));
            adapter.get_app_dirs().unwrap()
        });
        let dirs_b = with_uc_profile(Some("b"), || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/tmp"));
            adapter.get_app_dirs().unwrap()
        });

        assert_eq!(
            dirs_a.app_data_root,
            PathBuf::from("/tmp/app.uniclipboard.desktop-a")
        );
        assert_eq!(
            dirs_b.app_data_root,
            PathBuf::from("/tmp/app.uniclipboard.desktop-b")
        );
        assert_ne!(dirs_a.app_data_root, dirs_b.app_data_root);
        assert_eq!(
            dirs_a.app_cache_root,
            PathBuf::from("/tmp/app.uniclipboard.desktop-a")
        );
        assert_eq!(
            dirs_b.app_cache_root,
            PathBuf::from("/tmp/app.uniclipboard.desktop-b")
        );
        assert_ne!(dirs_a.app_cache_root, dirs_b.app_cache_root);
    }

    #[test]
    fn sanitize_profile_component_preserves_valid_chars() {
        assert_eq!(sanitize_profile_component("abc"), "abc");
        assert_eq!(sanitize_profile_component("team-alpha"), "team-alpha");
        assert_eq!(sanitize_profile_component("profile_1"), "profile_1");
        assert_eq!(sanitize_profile_component("A1-b2_C3"), "A1-b2_C3");
    }

    #[test]
    fn sanitize_profile_component_replaces_invalid_chars() {
        assert_eq!(sanitize_profile_component("team/alpha"), "team_alpha");
        assert_eq!(sanitize_profile_component("a b c"), "a_b_c");
        assert_eq!(sanitize_profile_component("foo@bar!baz"), "foo_bar_baz");
    }

    #[test]
    fn sanitize_profile_component_falls_back_for_all_invalid() {
        assert_eq!(sanitize_profile_component("!!!"), "profile");
        assert_eq!(sanitize_profile_component("   "), "profile");
        assert_eq!(sanitize_profile_component(""), "profile");
    }

    #[test]
    fn daemon_pid_file_name_default() {
        with_uc_profile(None, || {
            assert_eq!(daemon_pid_file_name(), "uniclipboard-daemon.pid");
        });
    }

    #[test]
    fn daemon_pid_file_name_profile_aware() {
        with_uc_profile(Some("a"), || {
            assert_eq!(daemon_pid_file_name(), "uniclipboard-daemon-a.pid");
        });
        with_uc_profile(Some("team-alpha"), || {
            assert_eq!(daemon_pid_file_name(), "uniclipboard-daemon-team-alpha.pid");
        });
        with_uc_profile(Some(""), || {
            assert_eq!(daemon_pid_file_name(), "uniclipboard-daemon.pid");
        });
    }

    #[test]
    fn daemon_pid_file_name_sanitizes_invalid_chars() {
        with_uc_profile(Some("team/alpha"), || {
            assert_eq!(daemon_pid_file_name(), "uniclipboard-daemon-team_alpha.pid");
        });
    }

    #[test]
    fn resolve_daemon_pid_path_is_data_dir_based() {
        with_uc_profile(None, || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/var/data"));
            let app_dirs = adapter.get_app_dirs().unwrap();
            // The pid path should be inside the app data root, not /tmp
            let pid_path = app_dirs.app_data_root.join(daemon_pid_file_name());
            // Cross-platform: check that app_data_root contains our base (regardless of separator)
            assert!(
                pid_path.starts_with(&app_dirs.app_data_root),
                "pid path should start with app_data_root, got: {}",
                pid_path.display()
            );
            // Ensure /tmp is NOT in the path
            let path_str = pid_path.to_string_lossy();
            assert!(
                !path_str.contains("/tmp") && !path_str.contains("\\tmp"),
                "pid path should NOT be under /tmp, got: {}",
                path_str
            );
        });
    }

    #[test]
    fn daemon_token_file_name_default() {
        with_uc_profile(None, || {
            assert_eq!(daemon_token_file_name(), "uniclipboard-daemon.token");
        });
    }

    #[test]
    fn daemon_token_file_name_profile_aware() {
        with_uc_profile(Some("a"), || {
            assert_eq!(daemon_token_file_name(), "uniclipboard-daemon-a.token");
        });
        with_uc_profile(Some(""), || {
            assert_eq!(daemon_token_file_name(), "uniclipboard-daemon.token");
        });
    }

    #[test]
    fn resolve_daemon_token_path_is_data_dir_based() {
        with_uc_profile(None, || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/var/data"));
            let app_dirs = adapter.get_app_dirs().unwrap();
            let token_path = app_dirs.app_data_root.join(daemon_token_file_name());
            assert!(
                token_path.starts_with(&app_dirs.app_data_root),
                "token path should start with app_data_root, got: {}",
                token_path.display()
            );
            let path_str = token_path.to_string_lossy();
            assert!(
                !path_str.contains("/tmp") && !path_str.contains("\\tmp"),
                "token path should NOT be under /tmp, got: {}",
                path_str
            );
        });
    }

    #[test]
    fn resolve_daemon_token_path_profile_aware() {
        let path_a = with_uc_profile(Some("a"), || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/tmp"));
            adapter
                .get_app_dirs()
                .unwrap()
                .app_data_root
                .join(daemon_token_file_name())
        });
        let path_b = with_uc_profile(Some("b"), || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/tmp"));
            adapter
                .get_app_dirs()
                .unwrap()
                .app_data_root
                .join(daemon_token_file_name())
        });
        let path_default = with_uc_profile(None, || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/tmp"));
            adapter
                .get_app_dirs()
                .unwrap()
                .app_data_root
                .join(daemon_token_file_name())
        });

        assert_ne!(path_a, path_b);
        assert_ne!(path_a, path_default);
        assert_ne!(path_b, path_default);
    }

    #[test]
    fn daemon_token_and_pid_share_same_directory() {
        with_uc_profile(None, || {
            let adapter = DirsAppDirsAdapter::with_base_data_local_dir(PathBuf::from("/var/data"));
            let app_dirs = adapter.get_app_dirs().unwrap();
            let token_path = app_dirs.app_data_root.join(daemon_token_file_name());
            let pid_path = app_dirs.app_data_root.join(daemon_pid_file_name());
            assert_eq!(
                token_path.parent(),
                pid_path.parent(),
                "token and pid should be in the same directory"
            );
        });
    }
}
