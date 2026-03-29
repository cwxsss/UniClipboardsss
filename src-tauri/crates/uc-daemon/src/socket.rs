//! Shared daemon socket and loopback address resolution.

use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

const DAEMON_FILE_STEM: &str = "uniclipboard-daemon";
const SOCKET_EXTENSION: &str = "sock";
const TOKEN_EXTENSION: &str = "token";
pub const DEFAULT_HTTP_HOST: &str = "127.0.0.1";
pub const DEFAULT_HTTP_PORT: u16 = 42715;
const PROFILE_A_HTTP_PORT: u16 = 42716;
const PROFILE_B_HTTP_PORT: u16 = 42717;
const PROFILE_HTTP_PORT_START: u16 = 42718;

#[cfg(unix)]
const MAX_SOCKET_PATH_BYTES: usize = 103;

/// Resolve the daemon RPC socket path.
#[cfg(unix)]
pub fn resolve_daemon_socket_path() -> PathBuf {
    let xdg_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();
    let base = sanitize_xdg_runtime_dir(xdg_runtime_dir.as_deref());
    resolve_daemon_socket_path_from(base.as_deref())
}

/// Resolve the daemon RPC socket path.
#[cfg(not(unix))]
pub fn resolve_daemon_socket_path() -> PathBuf {
    std::env::temp_dir().join(socket_file_name())
}

/// Resolve a daemon-local token file path within the provided base directory.
pub fn resolve_daemon_token_path_from(base: &Path) -> PathBuf {
    base.join(token_file_name())
}

/// Resolve the loopback-only daemon HTTP listen address.
pub fn resolve_daemon_http_addr() -> SocketAddr {
    try_resolve_daemon_http_addr()
        .expect("daemon http address resolution should stay within loopback port range")
}

/// Resolve the loopback-only daemon HTTP listen address with explicit error propagation.
pub fn try_resolve_daemon_http_addr() -> Result<SocketAddr> {
    Ok(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        resolve_daemon_http_port()?,
    ))
}

#[cfg(unix)]
fn sanitize_xdg_runtime_dir(xdg: Option<&str>) -> Option<PathBuf> {
    let value = xdg?.trim();
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

#[cfg(unix)]
fn resolve_daemon_socket_path_from(base: Option<&Path>) -> PathBuf {
    let fallback = Path::new("/tmp");
    let candidate_base = base.unwrap_or(fallback);
    let socket_name = socket_file_name();
    let candidate = candidate_base.join(&socket_name);

    if socket_path_byte_len(&candidate) <= MAX_SOCKET_PATH_BYTES {
        return candidate;
    }

    let fallback_path = fallback.join(&socket_name);
    tracing::warn!(
        socket_path = %candidate.display(),
        fallback_path = %fallback_path.display(),
        socket_path_bytes = socket_path_byte_len(&candidate),
        max_socket_path_bytes = MAX_SOCKET_PATH_BYTES,
        "daemon socket path exceeds unix socket byte limit; falling back to /tmp"
    );
    fallback_path
}

#[cfg(unix)]
fn socket_path_byte_len(path: &Path) -> usize {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().len()
}

#[cfg(not(unix))]
fn socket_path_byte_len(path: &Path) -> usize {
    path.as_os_str().len()
}

fn socket_file_name() -> String {
    daemon_file_name(SOCKET_EXTENSION)
}

fn token_file_name() -> String {
    daemon_file_name(TOKEN_EXTENSION)
}

fn daemon_file_name(extension: &str) -> String {
    match resolved_uc_profile() {
        Some(profile) => format!(
            "{DAEMON_FILE_STEM}-{}.{}",
            sanitize_profile_component(&profile),
            extension
        ),
        None => format!("{DAEMON_FILE_STEM}.{extension}"),
    }
}

fn resolved_uc_profile() -> Option<String> {
    let profile = std::env::var("UC_PROFILE").ok()?;
    let trimmed = profile.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

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

fn resolve_daemon_http_port() -> Result<u16> {
    match resolved_uc_profile().as_deref() {
        None => Ok(DEFAULT_HTTP_PORT),
        Some(profile) if profile.eq_ignore_ascii_case("a") => Ok(PROFILE_A_HTTP_PORT),
        Some(profile) if profile.eq_ignore_ascii_case("b") => Ok(PROFILE_B_HTTP_PORT),
        Some(profile) => resolve_hashed_profile_http_port(profile),
    }
}

fn resolve_hashed_profile_http_port(profile: &str) -> Result<u16> {
    let slot_count = u32::from(u16::MAX) - u32::from(PROFILE_HTTP_PORT_START) + 1;
    let hash = stable_profile_hash(profile);
    let offset = (hash % u64::from(slot_count)) as u16;

    PROFILE_HTTP_PORT_START
        .checked_add(offset)
        .with_context(|| {
            format!(
            "profile-derived daemon HTTP port overflowed reserved range for UC_PROFILE={profile}"
        )
        })
}

fn stable_profile_hash(profile: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    profile.as_bytes().iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn with_uc_profile<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var("UC_PROFILE").ok();

        match value {
            Some(profile) => std::env::set_var("UC_PROFILE", profile),
            None => std::env::remove_var("UC_PROFILE"),
        }

        let result = f();

        match previous {
            Some(profile) => std::env::set_var("UC_PROFILE", profile),
            None => std::env::remove_var("UC_PROFILE"),
        }

        result
    }

    #[test]
    fn test_socket_path_ends_with_sock() {
        let path = with_uc_profile(None, resolve_daemon_socket_path);
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("uniclipboard-daemon.sock")
        );
    }

    #[test]
    fn test_token_path_ends_with_token() {
        let path = with_uc_profile(None, || {
            resolve_daemon_token_path_from(Path::new("/tmp/uniclipboard"))
        });
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("uniclipboard-daemon.token")
        );
    }

    #[test]
    fn test_profiled_socket_paths_use_distinct_file_names() {
        let path_a = with_uc_profile(Some("a"), resolve_daemon_socket_path);
        let path_b = with_uc_profile(Some("b"), resolve_daemon_socket_path);

        assert_eq!(
            path_a.file_name().and_then(|name| name.to_str()),
            Some("uniclipboard-daemon-a.sock")
        );
        assert_eq!(
            path_b.file_name().and_then(|name| name.to_str()),
            Some("uniclipboard-daemon-b.sock")
        );
        assert_ne!(path_a, path_b);
    }

    #[test]
    fn test_profiled_token_paths_use_distinct_file_names() {
        let path_a = with_uc_profile(Some("a"), || {
            resolve_daemon_token_path_from(Path::new("/tmp/uniclipboard"))
        });
        let path_b = with_uc_profile(Some("b"), || {
            resolve_daemon_token_path_from(Path::new("/tmp/uniclipboard"))
        });

        assert_eq!(
            path_a.file_name().and_then(|name| name.to_str()),
            Some("uniclipboard-daemon-a.token")
        );
        assert_eq!(
            path_b.file_name().and_then(|name| name.to_str()),
            Some("uniclipboard-daemon-b.token")
        );
        assert_ne!(path_a, path_b);
    }

    #[test]
    fn test_http_addr_is_loopback() {
        let addr = with_uc_profile(None, resolve_daemon_http_addr);
        assert_eq!(addr.ip().to_string(), DEFAULT_HTTP_HOST);
        assert_eq!(addr.port(), DEFAULT_HTTP_PORT);
    }

    #[test]
    fn test_profiled_http_addr_uses_stable_distinct_ports() {
        let default_addr = with_uc_profile(None, resolve_daemon_http_addr);
        let addr_a = with_uc_profile(Some("a"), resolve_daemon_http_addr);
        let addr_b = with_uc_profile(Some("b"), resolve_daemon_http_addr);
        let addr_team = with_uc_profile(Some("team-alpha"), resolve_daemon_http_addr);
        let addr_team_repeat = with_uc_profile(Some("team-alpha"), resolve_daemon_http_addr);

        assert_eq!(default_addr.port(), 42715);
        assert_eq!(addr_a.port(), 42716);
        assert_eq!(addr_b.port(), 42717);
        assert_ne!(addr_team.port(), default_addr.port());
        assert_ne!(addr_team.port(), addr_a.port());
        assert_ne!(addr_team.port(), addr_b.port());
        assert_eq!(addr_team.port(), addr_team_repeat.port());
    }

    #[cfg(unix)]
    #[test]
    fn test_socket_path_length() {
        let path = with_uc_profile(None, || resolve_daemon_socket_path_from(None));
        assert!(socket_path_byte_len(&path) <= MAX_SOCKET_PATH_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn test_xdg_runtime_dir_override() {
        let path = with_uc_profile(None, || {
            resolve_daemon_socket_path_from(Some(Path::new("/run/user/1000")))
        });
        assert_eq!(
            path,
            PathBuf::from("/run/user/1000").join("uniclipboard-daemon.sock")
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_sanitize_xdg_none() {
        assert_eq!(sanitize_xdg_runtime_dir(None), None);
    }

    #[cfg(unix)]
    #[test]
    fn test_sanitize_xdg_empty() {
        assert_eq!(sanitize_xdg_runtime_dir(Some("")), None);
    }

    #[cfg(unix)]
    #[test]
    fn test_sanitize_xdg_whitespace_only() {
        assert_eq!(sanitize_xdg_runtime_dir(Some("   ")), None);
    }

    #[cfg(unix)]
    #[test]
    fn test_sanitize_xdg_valid() {
        assert_eq!(
            sanitize_xdg_runtime_dir(Some("/run/user/1000")),
            Some(PathBuf::from("/run/user/1000"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_from_none() {
        let path = with_uc_profile(None, || resolve_daemon_socket_path_from(None));
        assert_eq!(path, PathBuf::from("/tmp").join("uniclipboard-daemon.sock"));
    }

    #[cfg(unix)]
    #[test]
    fn test_xdg_runtime_dir_too_long() {
        let long_base = PathBuf::from("/").join("a".repeat(90));
        let path = with_uc_profile(None, || {
            resolve_daemon_socket_path_from(Some(long_base.as_path()))
        });
        assert_eq!(path, PathBuf::from("/tmp").join("uniclipboard-daemon.sock"));
    }

    #[cfg(unix)]
    #[test]
    fn test_socket_path_boundary() {
        let max_base_len = MAX_SOCKET_PATH_BYTES - 1 - "uniclipboard-daemon.sock".len();
        let exact_base = PathBuf::from(format!("/{}", "x".repeat(max_base_len - 1)));
        let exact_path = with_uc_profile(None, || {
            resolve_daemon_socket_path_from(Some(exact_base.as_path()))
        });
        assert_eq!(socket_path_byte_len(&exact_path), MAX_SOCKET_PATH_BYTES);
        assert_eq!(exact_path, exact_base.join("uniclipboard-daemon.sock"));

        let too_long_base = PathBuf::from(format!("/{}", "x".repeat(max_base_len)));
        let too_long_path = with_uc_profile(None, || {
            resolve_daemon_socket_path_from(Some(too_long_base.as_path()))
        });
        assert_eq!(
            too_long_path,
            PathBuf::from("/tmp").join("uniclipboard-daemon.sock")
        );
    }

    #[test]
    fn test_profile_component_sanitizes_non_filename_characters() {
        let path = with_uc_profile(Some(" team/a\\b "), || {
            resolve_daemon_token_path_from(Path::new("/tmp/uniclipboard"))
        });

        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("uniclipboard-daemon-team_a_b.token")
        );
    }
}
