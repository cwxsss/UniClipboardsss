use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use uc_core::network::daemon_api_strings::pairing_error_code;

use crate::pairing::host::DaemonPairingHostError;

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorResponse {
    pub code: String,
    pub message: String,
}

impl ApiErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error".to_string(),
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "bad_request".to_string(),
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            code: "unauthorized".to_string(),
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiErrorResponse {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error".to_string(),
            message: message.into(),
        }
    }

    pub fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "runtime_unavailable".to_string(),
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request".to_string(),
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict".to_string(),
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized".to_string(),
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found".to_string(),
            message: message.into(),
        }
    }

    /// Creates an ApiError from a DaemonPairingHostError, preserving the pairing-specific
    /// error code instead of using a generic one.
    pub fn from_pairing_error(error: DaemonPairingHostError) -> Self {
        match error {
            DaemonPairingHostError::ActivePairingSessionExists => Self {
                status: StatusCode::CONFLICT,
                code: pairing_error_code::ACTIVE_SESSION_EXISTS.to_string(),
                message: "active pairing session exists".to_string(),
            },
            DaemonPairingHostError::HostNotDiscoverable => Self {
                status: StatusCode::BAD_REQUEST,
                code: pairing_error_code::HOST_NOT_DISCOVERABLE.to_string(),
                message: "host not discoverable".to_string(),
            },
            DaemonPairingHostError::NoLocalPairingParticipantReady => Self {
                status: StatusCode::BAD_REQUEST,
                code: pairing_error_code::NO_LOCAL_PARTICIPANT.to_string(),
                message: "no local pairing participant ready".to_string(),
            },
            DaemonPairingHostError::SessionNotFound(_) => Self {
                status: StatusCode::NOT_FOUND,
                code: pairing_error_code::SESSION_NOT_FOUND.to_string(),
                message: "pairing session not found".to_string(),
            },
            DaemonPairingHostError::Internal(message) => {
                tracing::error!(error = %message, "daemon pairing command failed");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: pairing_error_code::INTERNAL.to_string(),
                    message,
                }
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ApiErrorResponse {
            code: self.code,
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}
