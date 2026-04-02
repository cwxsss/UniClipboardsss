use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use uc_platform::resolve_daemon_pid_path;

/// Resolve the profile-aware PID metadata path for the expected local daemon.
pub fn resolve_pid_path() -> PathBuf {
    resolve_daemon_pid_path().expect("data directory must be available to write daemon pid file")
}

/// Resolve the PID path using a custom base directory (test-only).
#[cfg(test)]
pub fn resolve_pid_path_for_testing(base: std::path::PathBuf) -> std::path::PathBuf {
    uc_platform::app_dirs::resolve_daemon_pid_path_for_testing(base)
        .expect("test base directory must be valid")
}

/// Persist the current daemon PID for the expected local daemon endpoint.
pub fn write_current_pid() -> Result<u32> {
    let pid_path = resolve_pid_path();
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

/// Remove the expected local daemon PID metadata file.
pub fn remove_pid_file() -> Result<()> {
    let pid_path = resolve_pid_path();
    match fs::remove_file(&pid_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow::Error::new(error).context(format!(
            "failed to remove daemon pid file {}",
            pid_path.display()
        ))),
    }
}

/// Read the stored daemon PID for the expected local daemon endpoint.
pub fn read_pid_file() -> Result<Option<u32>> {
    let pid_path = resolve_pid_path();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    /// Temporarily sets `UC_PROFILE` and restores it after `f` completes.
    fn with_uc_profile<T>(profile: Option<&str>, f: impl FnOnce() -> T) -> T {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var("UC_PROFILE").ok();
        match profile {
            Some(p) => std::env::set_var("UC_PROFILE", p),
            None => std::env::remove_var("UC_PROFILE"),
        }
        let result = f();
        match previous {
            Some(p) => std::env::set_var("UC_PROFILE", p),
            None => std::env::remove_var("UC_PROFILE"),
        }
        result
    }

    #[test]
    fn pid_path_tracks_uc_profile() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");
        let base: std::path::PathBuf = tempdir.path().into();

        let path_a = with_uc_profile(Some("a"), || resolve_pid_path_for_testing(base.clone()));
        let path_b = with_uc_profile(Some("b"), || resolve_pid_path_for_testing(base.clone()));

        assert_eq!(
            path_a.file_name().and_then(std::ffi::OsStr::to_str),
            Some("uniclipboard-daemon-a.pid")
        );
        assert_eq!(
            path_b.file_name().and_then(std::ffi::OsStr::to_str),
            Some("uniclipboard-daemon-b.pid")
        );
        assert_ne!(path_a, path_b);
    }

    #[test]
    fn write_current_pid_persists_profile_aware_pid_file() {
        with_uc_profile(Some("a"), || {
            let pid_path = resolve_pid_path();
            if let Some(parent) = pid_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&pid_path, std::process::id().to_string()).unwrap();

            let stored_pid = read_pid_file()
                .expect("pid file should be readable")
                .expect("pid file should exist");
            assert_eq!(stored_pid, std::process::id());

            std::fs::remove_file(&pid_path).ok();
        });
    }

    #[test]
    fn remove_pid_file_deletes_existing_pid_metadata() {
        with_uc_profile(Some("b"), || {
            let pid_path = resolve_pid_path();
            if let Some(parent) = pid_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&pid_path, std::process::id().to_string()).unwrap();

            remove_pid_file().expect("pid file should be removed");
            assert!(read_pid_file().expect("pid read should succeed").is_none());
        });
    }
}
