use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::api::auth::resolve_daemon_token_path;
use crate::socket::resolve_daemon_socket_path;

/// Resolve the profile-aware PID metadata path for the expected local daemon.
pub fn resolve_daemon_pid_path() -> PathBuf {
    let socket_path = resolve_daemon_socket_path();
    let base_dir = socket_path.parent().unwrap_or_else(|| Path::new("/tmp"));
    resolve_daemon_token_path(base_dir).with_extension("pid")
}

/// Persist the current daemon PID for the expected local daemon endpoint.
pub fn write_current_pid() -> Result<u32> {
    let pid_path = resolve_daemon_pid_path();
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
    let pid_path = resolve_daemon_pid_path();
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
    let pid_path = resolve_daemon_pid_path();
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

    fn with_daemon_env<T>(
        profile: Option<&str>,
        xdg_runtime_dir: Option<&Path>,
        f: impl FnOnce() -> T,
    ) -> T {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_profile = std::env::var("UC_PROFILE").ok();
        let previous_xdg_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();

        match profile {
            Some(profile) => std::env::set_var("UC_PROFILE", profile),
            None => std::env::remove_var("UC_PROFILE"),
        }
        match xdg_runtime_dir {
            Some(path) => std::env::set_var("XDG_RUNTIME_DIR", path),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }

        let result = f();

        match previous_profile {
            Some(profile) => std::env::set_var("UC_PROFILE", profile),
            None => std::env::remove_var("UC_PROFILE"),
        }
        match previous_xdg_runtime_dir {
            Some(path) => std::env::set_var("XDG_RUNTIME_DIR", path),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }

        result
    }

    #[test]
    fn pid_path_tracks_uc_profile() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");

        let path_a = with_daemon_env(Some("a"), Some(tempdir.path()), resolve_daemon_pid_path);
        let path_b = with_daemon_env(Some("b"), Some(tempdir.path()), resolve_daemon_pid_path);

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
        let tempdir = tempfile::tempdir().expect("tempdir should be created");

        let written_pid = with_daemon_env(Some("a"), Some(tempdir.path()), || {
            write_current_pid().expect("pid file should be written")
        });
        let stored_pid = with_daemon_env(Some("a"), Some(tempdir.path()), || {
            read_pid_file()
                .expect("pid file should be readable")
                .expect("pid file should exist")
        });

        assert_eq!(written_pid, std::process::id());
        assert_eq!(stored_pid, std::process::id());
    }

    #[test]
    fn remove_pid_file_deletes_existing_pid_metadata() {
        let tempdir = tempfile::tempdir().expect("tempdir should be created");

        with_daemon_env(Some("b"), Some(tempdir.path()), || {
            write_current_pid().expect("pid file should be written");
            remove_pid_file().expect("pid file should be removed");
            assert!(read_pid_file().expect("pid read should succeed").is_none());
        });
    }
}
