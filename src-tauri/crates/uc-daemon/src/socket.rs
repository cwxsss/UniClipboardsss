//! Shared daemon HTTP address resolution.

use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

pub use uc_platform::resolve_daemon_pid_path;
pub use uc_platform::resolve_daemon_token_path;

pub const DEFAULT_HTTP_HOST: &str = "127.0.0.1";
pub const DEFAULT_HTTP_PORT: u16 = 42715;
const PROFILE_A_HTTP_PORT: u16 = 42716;
const PROFILE_B_HTTP_PORT: u16 = 42717;
const PROFILE_HTTP_PORT_START: u16 = 42718;

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

fn resolve_daemon_http_port() -> Result<u16> {
    match std::env::var("UC_PROFILE") {
        Ok(profile) if profile.trim().is_empty() => Ok(DEFAULT_HTTP_PORT),
        Ok(profile) if profile.eq_ignore_ascii_case("a") => Ok(PROFILE_A_HTTP_PORT),
        Ok(profile) if profile.eq_ignore_ascii_case("b") => Ok(PROFILE_B_HTTP_PORT),
        Ok(profile) => resolve_hashed_profile_http_port(&profile),
        Err(_) => Ok(DEFAULT_HTTP_PORT),
    }
}

fn resolve_hashed_profile_http_port(profile: &str) -> Result<u16> {
    let slot_count = u32::from(u16::MAX) - u32::from(PROFILE_HTTP_PORT_START) + 1;
    let hash = stable_profile_hash(profile);
    let offset = (hash % u64::from(slot_count)) as u16;

    PROFILE_HTTP_PORT_START.checked_add(offset).ok_or_else(|| {
        anyhow::anyhow!(
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
    use uc_platform::app_dirs::resolve_daemon_token_path_for_testing;

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

    #[test]
    fn test_token_path_uses_testing_override() {
        let path = with_uc_profile(None, || {
            resolve_daemon_token_path_for_testing(std::path::PathBuf::from("/tmp"))
                .expect("should resolve")
        });
        assert!(
            path.to_string_lossy().contains("/tmp/"),
            "token path should be under /tmp/, got: {}",
            path.display()
        );
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("uniclipboard-daemon.token")
        );
    }

    #[test]
    fn test_profiled_token_paths_use_distinct_directories() {
        let path_a = with_uc_profile(Some("a"), || {
            resolve_daemon_token_path_for_testing(std::path::PathBuf::from("/tmp"))
                .expect("should resolve")
        });
        let path_b = with_uc_profile(Some("b"), || {
            resolve_daemon_token_path_for_testing(std::path::PathBuf::from("/tmp"))
                .expect("should resolve")
        });
        let path_default = with_uc_profile(None, || {
            resolve_daemon_token_path_for_testing(std::path::PathBuf::from("/tmp"))
                .expect("should resolve")
        });

        assert_ne!(path_a, path_b);
        assert_ne!(path_a, path_default);
        assert_ne!(path_b, path_default);
    }
}
