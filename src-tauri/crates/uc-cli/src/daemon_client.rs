use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use uc_daemon::api::auth::resolve_daemon_token_path;
use uc_daemon::api::pairing::{
    AckedPairingCommandResponse, PairingGuiLeaseRequest, PairingSessionCommandRequest,
    VerifyPairingRequest,
};
use uc_daemon::api::types::{
    PairedDeviceDto, PeerSnapshotDto, SetupActionAckResponse, SetupResetResponse,
    SetupSelectPeerRequest, SetupStateResponse, SetupSubmitPassphraseRequest, StatusResponse,
};
use uc_daemon::socket::{resolve_daemon_socket_path, try_resolve_daemon_http_addr};

const DEFAULT_QUERY_TIMEOUT_SECS: u64 = 2;
const SETUP_ACTION_TIMEOUT_SECS: u64 = 15;
const ENV_BASE_URL: &str = "UNICLIPBOARD_DAEMON_BASE_URL";
const ENV_TOKEN_PATH: &str = "UNICLIPBOARD_DAEMON_TOKEN_PATH";

pub struct DaemonHttpClient {
    http: Client,
    base_url: String,
    token: String,
}

#[derive(Debug)]
pub enum DaemonClientError {
    Unreachable(anyhow::Error),
    Unauthorized,
    Initialization(anyhow::Error),
    UnexpectedStatus { status: StatusCode, body: String },
    InvalidResponse(anyhow::Error),
}

impl fmt::Display for DaemonClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable(error) => write!(f, "daemon unreachable: {error}"),
            Self::Unauthorized => write!(f, "daemon rejected request: unauthorized"),
            Self::Initialization(error) => write!(f, "failed to initialize daemon client: {error}"),
            Self::UnexpectedStatus { status, body } => {
                if body.is_empty() {
                    write!(f, "daemon returned unexpected status {status}")
                } else {
                    write!(f, "daemon returned unexpected status {status}: {body}")
                }
            }
            Self::InvalidResponse(error) => write!(f, "daemon returned invalid response: {error}"),
        }
    }
}

impl std::error::Error for DaemonClientError {}

impl DaemonHttpClient {
    pub fn new() -> Result<Self, DaemonClientError> {
        let base_url = resolve_base_url_for_client()?;
        let token_path = resolve_token_path();
        let token = std::fs::read_to_string(&token_path).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => {
                DaemonClientError::Unreachable(anyhow::Error::new(error).context(format!(
                    "daemon auth token not found at {}",
                    token_path.display()
                )))
            }
            _ => DaemonClientError::Initialization(anyhow::Error::new(error).context(format!(
                "failed to read daemon auth token at {}",
                token_path.display()
            ))),
        })?;
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(DaemonClientError::Initialization(anyhow::anyhow!(
                "daemon auth token at {} is empty",
                token_path.display()
            )));
        }

        Self::from_parts(base_url, token)
    }

    pub async fn get_status(&self) -> Result<StatusResponse, DaemonClientError> {
        self.get_json("/status").await
    }

    pub async fn get_peers(&self) -> Result<Vec<PeerSnapshotDto>, DaemonClientError> {
        self.get_json("/peers").await
    }

    pub async fn get_paired_devices(&self) -> Result<Vec<PairedDeviceDto>, DaemonClientError> {
        self.get_json("/paired-devices").await
    }

    pub async fn get_setup_state(&self) -> Result<SetupStateResponse, DaemonClientError> {
        self.get_json("/setup/state").await
    }

    pub async fn start_setup_host(&self) -> Result<SetupActionAckResponse, DaemonClientError> {
        self.post_without_body("/setup/host", setup_action_timeout())
            .await
    }

    pub async fn start_setup_join(&self) -> Result<SetupActionAckResponse, DaemonClientError> {
        self.post_without_body("/setup/join", setup_action_timeout())
            .await
    }

    pub async fn select_setup_peer(
        &self,
        peer_id: String,
    ) -> Result<SetupActionAckResponse, DaemonClientError> {
        self.post_json(
            "/setup/select-peer",
            &SetupSelectPeerRequest { peer_id },
            setup_action_timeout(),
        )
        .await
    }

    pub async fn confirm_setup_peer(&self) -> Result<SetupActionAckResponse, DaemonClientError> {
        self.post_without_body("/setup/confirm-peer", setup_action_timeout())
            .await
    }

    pub async fn accept_pairing_session(
        &self,
        session_id: String,
    ) -> Result<AckedPairingCommandResponse, DaemonClientError> {
        self.post_json(
            "/pairing/accept",
            &PairingSessionCommandRequest { session_id },
            setup_action_timeout(),
        )
        .await
    }

    pub async fn verify_pairing_session(
        &self,
        session_id: String,
        pin_matches: bool,
    ) -> Result<AckedPairingCommandResponse, DaemonClientError> {
        self.post_json(
            &format!("/pairing/sessions/{session_id}/verify"),
            &VerifyPairingRequest { pin_matches },
            setup_action_timeout(),
        )
        .await
    }

    pub async fn submit_setup_passphrase(
        &self,
        passphrase: String,
    ) -> Result<SetupActionAckResponse, DaemonClientError> {
        self.post_json(
            "/setup/submit-passphrase",
            &SetupSubmitPassphraseRequest { passphrase },
            setup_action_timeout(),
        )
        .await
    }

    pub async fn cancel_setup(&self) -> Result<SetupActionAckResponse, DaemonClientError> {
        self.post_without_body("/setup/cancel", setup_action_timeout())
            .await
    }

    pub async fn reset_setup(&self) -> Result<SetupResetResponse, DaemonClientError> {
        self.post_without_body("/setup/reset", setup_action_timeout())
            .await
    }

    pub async fn set_pairing_gui_lease(&self, enabled: bool) -> Result<(), DaemonClientError> {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, "/pairing/gui/lease"))
            .timeout(setup_action_timeout())
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token),
            )
            .json(&PairingGuiLeaseRequest {
                enabled,
                lease_ttl_ms: None,
            })
            .send()
            .await
            .map_err(map_reqwest_error)?;

        decode_empty_response(response).await
    }

    fn from_parts(base_url: String, token: String) -> Result<Self, DaemonClientError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_QUERY_TIMEOUT_SECS))
            .build()
            .map_err(|error| {
                DaemonClientError::Initialization(
                    anyhow::Error::new(error).context("failed to build daemon HTTP client"),
                )
            })?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
        })
    }

    async fn get_json<T>(&self, path: &str) -> Result<T, DaemonClientError>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token),
            )
            .send()
            .await
            .map_err(map_reqwest_error)?;

        decode_json_response(response).await
    }

    async fn post_without_body<T>(
        &self,
        path: &str,
        timeout: Duration,
    ) -> Result<T, DaemonClientError>
    where
        T: DeserializeOwned,
    {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .timeout(timeout)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token),
            )
            .send()
            .await
            .map_err(map_reqwest_error)?;

        decode_json_response(response).await
    }

    async fn post_json<T, B>(
        &self,
        path: &str,
        body: &B,
        timeout: Duration,
    ) -> Result<T, DaemonClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let response = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .timeout(timeout)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.token),
            )
            .json(body)
            .send()
            .await
            .map_err(map_reqwest_error)?;

        decode_json_response(response).await
    }
}

async fn decode_json_response<T>(response: reqwest::Response) -> Result<T, DaemonClientError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| DaemonClientError::InvalidResponse(error.into()))?;

    if status == StatusCode::UNAUTHORIZED {
        return Err(DaemonClientError::Unauthorized);
    }

    if !status.is_success() {
        return Err(DaemonClientError::UnexpectedStatus { status, body });
    }

    serde_json::from_str(&body).map_err(|error| {
        DaemonClientError::InvalidResponse(anyhow::Error::new(error).context(body))
    })
}

async fn decode_empty_response(response: reqwest::Response) -> Result<(), DaemonClientError> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| DaemonClientError::InvalidResponse(error.into()))?;

    if status == StatusCode::UNAUTHORIZED {
        return Err(DaemonClientError::Unauthorized);
    }

    if !status.is_success() {
        return Err(DaemonClientError::UnexpectedStatus { status, body });
    }

    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
fn resolve_base_url() -> String {
    resolve_base_url_for_client()
        .expect("daemon base url resolution should stay within reserved loopback port range")
}

fn resolve_base_url_for_client() -> Result<String, DaemonClientError> {
    if let Ok(value) = std::env::var(ENV_BASE_URL) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.trim_end_matches('/').to_string());
        }
    }

    let addr = try_resolve_daemon_http_addr().map_err(|error| {
        DaemonClientError::Initialization(
            error.context("failed to resolve profile-aware daemon HTTP address"),
        )
    })?;
    Ok(format!("http://{}:{}", addr.ip(), addr.port()))
}

fn resolve_token_path() -> PathBuf {
    if let Ok(value) = std::env::var(ENV_TOKEN_PATH) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let socket_path = resolve_daemon_socket_path();
    let token_base_dir = socket_path.parent().unwrap_or_else(|| Path::new("/tmp"));
    resolve_daemon_token_path(token_base_dir)
}

fn map_reqwest_error(error: reqwest::Error) -> DaemonClientError {
    if error.is_connect() || error.is_timeout() {
        return DaemonClientError::Unreachable(error.into());
    }

    DaemonClientError::InvalidResponse(error.into())
}

fn setup_action_timeout() -> Duration {
    Duration::from_secs(SETUP_ACTION_TIMEOUT_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct TestTempDir {
        path: PathBuf,
    }

    impl TestTempDir {
        fn new() -> Self {
            let unique = format!(
                "uc-cli-daemon-client-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("system time should be after epoch")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            fs::create_dir_all(&path).expect("test temp dir should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestTempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn with_daemon_env<T>(
        profile: Option<&str>,
        xdg_runtime_dir: Option<&Path>,
        base_url: Option<&str>,
        token_path: Option<&Path>,
        f: impl FnOnce() -> T,
    ) -> T {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_profile = std::env::var("UC_PROFILE").ok();
        let previous_xdg_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();
        let previous_base_url = std::env::var(ENV_BASE_URL).ok();
        let previous_token_path = std::env::var(ENV_TOKEN_PATH).ok();

        match profile {
            Some(profile) => std::env::set_var("UC_PROFILE", profile),
            None => std::env::remove_var("UC_PROFILE"),
        }
        match xdg_runtime_dir {
            Some(path) => std::env::set_var("XDG_RUNTIME_DIR", path),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
        match base_url {
            Some(value) => std::env::set_var(ENV_BASE_URL, value),
            None => std::env::remove_var(ENV_BASE_URL),
        }
        match token_path {
            Some(path) => std::env::set_var(ENV_TOKEN_PATH, path),
            None => std::env::remove_var(ENV_TOKEN_PATH),
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
        match previous_base_url {
            Some(value) => std::env::set_var(ENV_BASE_URL, value),
            None => std::env::remove_var(ENV_BASE_URL),
        }
        match previous_token_path {
            Some(path) => std::env::set_var(ENV_TOKEN_PATH, path),
            None => std::env::remove_var(ENV_TOKEN_PATH),
        }

        result
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer.windows(4).position(|window| window == b"\r\n\r\n")
    }

    #[test]
    fn resolve_base_url_uses_daemon_loopback_helper() {
        let base_url = with_daemon_env(None, None, None, None, resolve_base_url);
        let addr = with_daemon_env(None, None, None, None, || {
            uc_daemon::socket::resolve_daemon_http_addr()
        });
        assert_eq!(base_url, format!("http://{}:{}", addr.ip(), addr.port()));
    }

    #[test]
    fn resolve_token_path_uses_socket_parent_directory() {
        let token_path = with_daemon_env(None, None, None, None, resolve_token_path);
        let socket_path = with_daemon_env(None, None, None, None, resolve_daemon_socket_path);
        let expected =
            resolve_daemon_token_path(socket_path.parent().unwrap_or_else(|| Path::new("/tmp")));

        assert_eq!(token_path, expected);
    }

    #[test]
    fn resolve_base_url_is_profile_aware_by_default() {
        let base_url_a = with_daemon_env(Some("a"), None, None, None, resolve_base_url);
        let base_url_b = with_daemon_env(Some("b"), None, None, None, resolve_base_url);

        assert_eq!(base_url_a, "http://127.0.0.1:42716");
        assert_eq!(base_url_b, "http://127.0.0.1:42717");
        assert_ne!(base_url_a, base_url_b);
    }

    #[test]
    fn resolve_token_path_is_profile_aware_by_default() {
        let tempdir = TestTempDir::new();

        let token_path_a = with_daemon_env(
            Some("a"),
            Some(tempdir.path()),
            None,
            None,
            resolve_token_path,
        );
        let token_path_b = with_daemon_env(
            Some("b"),
            Some(tempdir.path()),
            None,
            None,
            resolve_token_path,
        );

        assert_eq!(
            token_path_a.file_name().and_then(std::ffi::OsStr::to_str),
            Some("uniclipboard-daemon-a.token")
        );
        assert_eq!(
            token_path_b.file_name().and_then(std::ffi::OsStr::to_str),
            Some("uniclipboard-daemon-b.token")
        );
        assert_ne!(token_path_a, token_path_b);
    }

    #[test]
    fn explicit_env_overrides_still_take_priority_over_profile_defaults() {
        let tempdir = TestTempDir::new();
        let explicit_token_path = tempdir.path().join("explicit.token");

        let base_url = with_daemon_env(
            Some("a"),
            Some(tempdir.path()),
            Some("http://127.0.0.1:49999"),
            Some(explicit_token_path.as_path()),
            resolve_base_url,
        );
        let token_path = with_daemon_env(
            Some("a"),
            Some(tempdir.path()),
            Some("http://127.0.0.1:49999"),
            Some(explicit_token_path.as_path()),
            resolve_token_path,
        );

        assert_eq!(base_url, "http://127.0.0.1:49999");
        assert_eq!(token_path, explicit_token_path);
    }

    #[test]
    fn setup_request_payloads_serialize_as_camel_case() {
        let select_peer = serde_json::to_value(SetupSelectPeerRequest {
            peer_id: "peer-a".to_string(),
        })
        .expect("setup select peer request should serialize");
        let submit_passphrase = serde_json::to_value(SetupSubmitPassphraseRequest {
            passphrase: "secret".to_string(),
        })
        .expect("setup submit passphrase request should serialize");

        assert_eq!(select_peer["peerId"], "peer-a");
        assert!(select_peer.get("peer_id").is_none());
        assert_eq!(submit_passphrase["passphrase"], "secret");
    }

    #[tokio::test]
    async fn set_pairing_gui_lease_sends_default_ttl_field() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener should expose addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("connection should arrive");
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            let mut expected_total_len = None;

            loop {
                let read = stream
                    .read(&mut buffer)
                    .await
                    .expect("request should be readable");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);

                if expected_total_len.is_none() {
                    if let Some(header_end) = find_header_end(&request) {
                        let headers = String::from_utf8_lossy(&request[..header_end]).to_string();
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.trim()
                                    .eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        expected_total_len = Some(header_end + 4 + content_length);
                    }
                }

                if let Some(expected_total_len) = expected_total_len {
                    if request.len() >= expected_total_len {
                        break;
                    }
                }
            }

            let header_end = find_header_end(&request).expect("request should include headers");
            let request_text =
                String::from_utf8(request).expect("request bytes should be valid utf-8");
            let request_lower = request_text.to_ascii_lowercase();
            let body = &request_text[header_end + 4..];
            let payload: serde_json::Value =
                serde_json::from_str(body).expect("request body should be valid json");

            assert!(
                request_text.starts_with("POST /pairing/gui/lease HTTP/1.1\r\n"),
                "unexpected request line: {request_text}"
            );
            assert!(
                request_lower.contains("\r\nauthorization: bearer test-token\r\n"),
                "authorization header should be present: {request_text}"
            );
            assert_eq!(payload["enabled"], true);
            assert!(payload["leaseTtlMs"].is_null());
            assert!(payload.get("lease_ttl_ms").is_none());

            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                )
                .await
                .expect("response should be writable");
        });

        let client = DaemonHttpClient::from_parts(format!("http://{addr}"), "test-token".into())
            .expect("client should build");

        client
            .set_pairing_gui_lease(true)
            .await
            .expect("request should succeed");

        server.await.expect("server should finish");
    }

    #[tokio::test]
    async fn submit_setup_passphrase_tolerates_slow_success_response() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let addr = listener.local_addr().expect("listener should expose addr");

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("connection should arrive");
            let mut buffer = vec![0; 4096];
            let _ = stream
                .read(&mut buffer)
                .await
                .expect("request should be readable");

            tokio::time::sleep(Duration::from_secs(3)).await;

            let body =
                r#"{"state":"ProcessingCreateSpace","sessionId":null,"nextStepHint":"completed"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("response should be writable");
        });

        let client = DaemonHttpClient::from_parts(format!("http://{addr}"), "test-token".into())
            .expect("client should build");

        let ack = client
            .submit_setup_passphrase("secret".to_string())
            .await
            .expect("slow setup response should not time out");

        assert_eq!(ack.next_step_hint, "completed");
        assert_eq!(
            ack.state,
            serde_json::Value::String("ProcessingCreateSpace".to_string())
        );

        server.await.expect("server should finish");
    }
}
