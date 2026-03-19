use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use reqwest::{Client, StatusCode};
use serde::de::DeserializeOwned;
use uc_daemon::api::auth::resolve_daemon_token_path;
use uc_daemon::api::types::{PairedDeviceDto, StatusResponse};
use uc_daemon::socket::{resolve_daemon_http_addr, resolve_daemon_socket_path};

const DEFAULT_TIMEOUT_SECS: u64 = 2;
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
        let base_url = resolve_base_url();
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

    pub async fn get_paired_devices(&self) -> Result<Vec<PairedDeviceDto>, DaemonClientError> {
        self.get_json("/paired-devices").await
    }

    fn from_parts(base_url: String, token: String) -> Result<Self, DaemonClientError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
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
}

fn resolve_base_url() -> String {
    if let Ok(value) = std::env::var(ENV_BASE_URL) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.trim_end_matches('/').to_string();
        }
    }

    let addr = resolve_daemon_http_addr();
    format!("http://{}:{}", addr.ip(), addr.port())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_base_url_uses_daemon_loopback_helper() {
        let base_url = resolve_base_url();
        let addr = resolve_daemon_http_addr();
        assert_eq!(base_url, format!("http://{}:{}", addr.ip(), addr.port()));
    }

    #[test]
    fn resolve_token_path_uses_socket_parent_directory() {
        let token_path = resolve_token_path();
        let socket_path = resolve_daemon_socket_path();
        let expected =
            resolve_daemon_token_path(socket_path.parent().unwrap_or_else(|| Path::new("/tmp")));

        assert_eq!(token_path, expected);
    }
}
