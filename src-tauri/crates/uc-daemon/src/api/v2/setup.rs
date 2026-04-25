//! Stateless v2 setup pairing HTTP handlers (Slice4 Phase 3 T3.2).
//!
//! Six endpoints under `/v2/setup/*`, each a thin adapter that
//! translates a `SpaceSetupFacade` call into the wire DTOs declared
//! in `uc_daemon_contract::api::dto::v2::setup`. Errors map onto the
//! daemon-wide `ApiError` surface (400 / 409 / 500 / 503).

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use uc_application::facade::{
    CancelInvitationError, InitializeSpaceError, IssuePairingInvitationResult,
    QuerySetupStateError, RedeemPairingInvitationError, RedeemPairingInvitationResult,
    ResetSpaceError, SetupStateView, SpaceSetupFacade,
};
use uc_application::facade::{
    InitializeSpaceCommand, InitializeSpaceResult, RedeemPairingInvitationCommand,
};
use uc_core::crypto::domain::Passphrase;
use uc_core::pairing::invitation::InvitationCode;
use uc_daemon_contract::api::dto::v2::setup::{
    CurrentInvitation, InitializeSpaceRequest, InitializeSpaceResponse, IssueInvitationResponse,
    RedeemRequest, RedeemResponse, SetupStateResponse,
};
use uc_daemon_contract::constants::http_route_v2;

use crate::api::dto::error::ApiError;
use crate::api::server::DaemonApiState;

pub fn router() -> Router<DaemonApiState> {
    Router::new()
        .route(http_route_v2::SETUP_INITIALIZE, post(initialize))
        .route(
            http_route_v2::SETUP_ISSUE_INVITATION,
            post(issue_invitation),
        )
        .route(http_route_v2::SETUP_REDEEM, post(redeem))
        .route(http_route_v2::SETUP_CANCEL, post(cancel))
        .route(http_route_v2::SETUP_RESET, post(reset))
        .route(http_route_v2::SETUP_STATE, get(get_state))
}

fn require_facade(state: &DaemonApiState) -> Result<std::sync::Arc<SpaceSetupFacade>, ApiError> {
    state
        .space_setup_facade()
        .ok_or_else(|| ApiError::service_unavailable("space setup facade not assembled"))
}

// ---------------------------------------------------------------------------
// POST /v2/setup/initialize
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/v2/setup/initialize",
    tag = "setup-v2",
    request_body = InitializeSpaceRequest,
    responses(
        (status = 200, body = InitializeSpaceResponse),
        (status = 400, description = "Passphrase mismatch or device name missing", body = crate::api::dto::error::ApiErrorResponse),
        (status = 409, description = "Setup already completed", body = crate::api::dto::error::ApiErrorResponse),
        (status = 503, description = "Facade not assembled", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Internal error", body = crate::api::dto::error::ApiErrorResponse),
    ),
)]
pub(crate) async fn initialize(
    State(state): State<DaemonApiState>,
    Json(req): Json<InitializeSpaceRequest>,
) -> Result<Json<InitializeSpaceResponse>, ApiError> {
    let facade = require_facade(&state)?;
    let cmd = InitializeSpaceCommand {
        passphrase: Passphrase::new(req.passphrase),
        passphrase_confirm: Passphrase::new(req.passphrase_confirm),
        device_name: req.device_name,
    };
    let out = facade.initialize_space(cmd).await.map_err(map_init_err)?;
    Ok(Json(initialize_to_dto(out)))
}

fn map_init_err(err: InitializeSpaceError) -> ApiError {
    match err {
        InitializeSpaceError::PassphraseMismatch => {
            ApiError::bad_request("passphrase and confirmation do not match")
        }
        InitializeSpaceError::DeviceNameRequired => {
            ApiError::bad_request("device name is required")
        }
        InitializeSpaceError::AlreadyInitialized => {
            ApiError::conflict("space is already initialised")
        }
        InitializeSpaceError::AlreadySetup => {
            ApiError::conflict("setup has already been completed on this device")
        }
        InitializeSpaceError::StorageFailed(msg) => ApiError::internal(msg),
        InitializeSpaceError::Internal(msg) => ApiError::internal(msg),
    }
}

fn initialize_to_dto(out: InitializeSpaceResult) -> InitializeSpaceResponse {
    InitializeSpaceResponse {
        space_id: out.space_id.to_string(),
        self_device_id: out.self_device_id.to_string(),
        fingerprint: out.fingerprint.as_display().to_string(),
    }
}

// ---------------------------------------------------------------------------
// POST /v2/setup/issue-invitation
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/v2/setup/issue-invitation",
    tag = "setup-v2",
    responses(
        (status = 200, body = IssueInvitationResponse),
        (status = 503, description = "Facade not assembled or network not started", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Internal error", body = crate::api::dto::error::ApiErrorResponse),
    ),
)]
pub(crate) async fn issue_invitation(
    State(state): State<DaemonApiState>,
) -> Result<Json<IssueInvitationResponse>, ApiError> {
    let facade = require_facade(&state)?;
    let out = facade.issue_pairing_invitation().await.map_err(|err| {
        use uc_application::facade::IssuePairingInvitationError as E;
        match err {
            E::NetworkNotStarted => ApiError::service_unavailable("network is not started"),
            E::ServiceUnavailable => {
                ApiError::service_unavailable("pairing invitation service unavailable")
            }
            E::Internal(msg) => ApiError::internal(msg),
        }
    })?;
    Ok(Json(issue_to_dto(out)))
}

fn issue_to_dto(out: IssuePairingInvitationResult) -> IssueInvitationResponse {
    IssueInvitationResponse {
        code: out.code.as_str().to_string(),
        expires_at_ms: out.expires_at.timestamp_millis(),
    }
}

// ---------------------------------------------------------------------------
// POST /v2/setup/redeem
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/v2/setup/redeem",
    tag = "setup-v2",
    request_body = RedeemRequest,
    responses(
        (status = 200, body = RedeemResponse),
        (status = 400, description = "Invalid request", body = crate::api::dto::error::ApiErrorResponse),
        (status = 404, description = "Invitation not found / expired", body = crate::api::dto::error::ApiErrorResponse),
        (status = 503, description = "Sponsor unreachable / service unavailable", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Internal error", body = crate::api::dto::error::ApiErrorResponse),
    ),
)]
pub(crate) async fn redeem(
    State(state): State<DaemonApiState>,
    Json(req): Json<RedeemRequest>,
) -> Result<Json<RedeemResponse>, ApiError> {
    let facade = require_facade(&state)?;
    let cmd = RedeemPairingInvitationCommand {
        code: InvitationCode::new(req.code),
        passphrase: Passphrase::new(req.passphrase),
    };
    let out = facade
        .redeem_pairing_invitation(cmd)
        .await
        .map_err(map_redeem_err)?;
    Ok(Json(redeem_to_dto(out)))
}

fn map_redeem_err(err: RedeemPairingInvitationError) -> ApiError {
    use RedeemPairingInvitationError as E;
    match err {
        E::InvitationNotFound => ApiError::not_found("invitation not found"),
        E::InvitationExpired => ApiError::not_found("invitation has expired"),
        E::SponsorUnreachable => ApiError::service_unavailable("sponsor is not reachable"),
        E::ServiceUnavailable => {
            ApiError::service_unavailable("pairing invitation service unavailable")
        }
        E::PassphraseMismatch => ApiError::bad_request("wrong passphrase"),
        E::CorruptedKeyMaterial => ApiError::internal("space key material corrupted"),
        E::DeviceNameRequired => ApiError::bad_request("device name is required"),
        E::SponsorRejectedInvitation => {
            ApiError::conflict("sponsor did not recognise the invitation code")
        }
        E::SponsorDeclined => ApiError::conflict("sponsor declined the pairing request"),
        E::SponsorTimedOut => ApiError::service_unavailable("sponsor timed out the handshake"),
        E::SponsorInternal(msg) => ApiError::internal(format!("sponsor internal error: {msg}")),
        E::Timeout => ApiError::service_unavailable("pairing handshake timed out"),
        E::ConnectionLost => ApiError::service_unavailable("connection lost mid-handshake"),
        E::Internal(msg) => ApiError::internal(msg),
    }
}

fn redeem_to_dto(out: RedeemPairingInvitationResult) -> RedeemResponse {
    RedeemResponse {
        sponsor_device_id: out.sponsor_device_id.to_string(),
        sponsor_identity_fingerprint: out.sponsor_identity_fingerprint.as_display().to_string(),
        space_id: out.space_id.to_string(),
        self_device_id: out.self_device_id.to_string(),
        self_identity_fingerprint: out.self_identity_fingerprint.as_display().to_string(),
    }
}

// ---------------------------------------------------------------------------
// POST /v2/setup/cancel
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/v2/setup/cancel",
    tag = "setup-v2",
    responses(
        (status = 204, description = "Invitation cancelled"),
        (status = 409, description = "No in-flight invitation to cancel", body = crate::api::dto::error::ApiErrorResponse),
        (status = 503, description = "Facade not assembled", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Internal error", body = crate::api::dto::error::ApiErrorResponse),
    ),
)]
pub(crate) async fn cancel(State(state): State<DaemonApiState>) -> Result<StatusCode, ApiError> {
    let facade = require_facade(&state)?;
    facade.cancel_invitation().await.map_err(|err| match err {
        CancelInvitationError::NotIssued => ApiError::conflict("no in-flight invitation to cancel"),
        CancelInvitationError::Internal(msg) => ApiError::internal(msg),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// POST /v2/setup/reset
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/v2/setup/reset",
    tag = "setup-v2",
    responses(
        (status = 204, description = "Setup reset"),
        (status = 503, description = "Facade not assembled", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Storage failure", body = crate::api::dto::error::ApiErrorResponse),
    ),
)]
pub(crate) async fn reset(State(state): State<DaemonApiState>) -> Result<StatusCode, ApiError> {
    let facade = require_facade(&state)?;
    facade.reset().await.map_err(|err| match err {
        ResetSpaceError::StorageFailed(msg) => ApiError::internal(msg),
        ResetSpaceError::Internal(msg) => ApiError::internal(msg),
    })?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// GET /v2/setup/state
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/v2/setup/state",
    tag = "setup-v2",
    responses(
        (status = 200, body = SetupStateResponse),
        (status = 503, description = "Facade not assembled", body = crate::api::dto::error::ApiErrorResponse),
        (status = 500, description = "Storage failure", body = crate::api::dto::error::ApiErrorResponse),
    ),
)]
pub(crate) async fn get_state(
    State(state): State<DaemonApiState>,
) -> Result<Json<SetupStateResponse>, ApiError> {
    let facade = require_facade(&state)?;
    let view = facade.query_setup_state().await.map_err(|err| match err {
        QuerySetupStateError::StorageFailed(msg) => ApiError::internal(msg),
        QuerySetupStateError::Internal(msg) => ApiError::internal(msg),
    })?;
    Ok(Json(state_to_dto(view)))
}

fn state_to_dto(view: SetupStateView) -> SetupStateResponse {
    SetupStateResponse {
        has_completed: view.has_completed,
        current_invitation: view.current_invitation.map(|inv| CurrentInvitation {
            code: inv.code.as_str().to_string(),
            expires_at_ms: inv.expires_at.timestamp_millis(),
        }),
        device_name: view.device_name,
    }
}

#[cfg(test)]
mod tests {
    //! Handler-internal pure-function tests. End-to-end router /
    //! facade integration is covered downstream once T3.3 wires
    //! `SpaceSetupFacade` into the daemon assembly: building a real
    //! `DaemonApiState` requires the full bootstrap path, which is
    //! out of scope for T3.2.

    use super::*;

    use chrono::{DateTime, Utc};
    use uc_application::facade::CurrentInvitation as FacadeCurrentInvitation;
    use uc_core::ids::{DeviceId, SpaceId};
    use uc_core::pairing::invitation::InvitationCode;
    use uc_core::security::IdentityFingerprint;

    fn fixed_fp() -> IdentityFingerprint {
        IdentityFingerprint::from_raw_string("ABCDEFGHIJKLMNOP").unwrap()
    }

    #[test]
    fn initialize_to_dto_flattens_domain_types_to_strings() {
        let dto = initialize_to_dto(InitializeSpaceResult {
            space_id: SpaceId::from_str("space-1"),
            self_device_id: DeviceId::new("device-1"),
            fingerprint: fixed_fp(),
        });
        assert_eq!(dto.space_id, "space-1");
        assert_eq!(dto.self_device_id, "device-1");
        assert_eq!(dto.fingerprint, "ABCD-EFGH-IJKL-MNOP");
    }

    #[test]
    fn issue_to_dto_serialises_expiry_as_epoch_millis() {
        let expires = DateTime::parse_from_rfc3339("2026-04-25T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let dto = issue_to_dto(IssuePairingInvitationResult {
            code: InvitationCode::new("ABCD-1234"),
            expires_at: expires,
        });
        assert_eq!(dto.code, "ABCD-1234");
        assert_eq!(dto.expires_at_ms, expires.timestamp_millis());
    }

    #[test]
    fn redeem_to_dto_carries_both_sides() {
        let dto = redeem_to_dto(RedeemPairingInvitationResult {
            sponsor_device_id: DeviceId::new("sponsor-1"),
            sponsor_identity_fingerprint: fixed_fp(),
            space_id: SpaceId::from_str("space-1"),
            self_device_id: DeviceId::new("joiner-2"),
            self_identity_fingerprint: fixed_fp(),
        });
        assert_eq!(dto.sponsor_device_id, "sponsor-1");
        assert_eq!(dto.self_device_id, "joiner-2");
        assert_eq!(dto.space_id, "space-1");
        assert_eq!(dto.sponsor_identity_fingerprint, "ABCD-EFGH-IJKL-MNOP");
        assert_eq!(dto.self_identity_fingerprint, "ABCD-EFGH-IJKL-MNOP");
    }

    #[test]
    fn state_to_dto_with_pending_invitation() {
        let expires = DateTime::parse_from_rfc3339("2026-04-25T13:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let dto = state_to_dto(SetupStateView {
            has_completed: true,
            current_invitation: Some(FacadeCurrentInvitation {
                code: InvitationCode::new("WXYZ"),
                expires_at: expires,
            }),
            device_name: Some("MacBook".to_string()),
        });
        assert!(dto.has_completed);
        let inv = dto.current_invitation.expect("invitation present");
        assert_eq!(inv.code, "WXYZ");
        assert_eq!(inv.expires_at_ms, expires.timestamp_millis());
        assert_eq!(dto.device_name.as_deref(), Some("MacBook"));
    }

    #[test]
    fn state_to_dto_fresh_install_returns_none_branches() {
        let dto = state_to_dto(SetupStateView {
            has_completed: false,
            current_invitation: None,
            device_name: None,
        });
        assert!(!dto.has_completed);
        assert!(dto.current_invitation.is_none());
        assert!(dto.device_name.is_none());
    }

    #[test]
    fn map_init_err_branches() {
        let err = map_init_err(InitializeSpaceError::PassphraseMismatch);
        assert_eq!(err.status.as_u16(), 400);
        let err = map_init_err(InitializeSpaceError::AlreadyInitialized);
        assert_eq!(err.status.as_u16(), 409);
        let err = map_init_err(InitializeSpaceError::AlreadySetup);
        assert_eq!(err.status.as_u16(), 409);
        let err = map_init_err(InitializeSpaceError::DeviceNameRequired);
        assert_eq!(err.status.as_u16(), 400);
        let err = map_init_err(InitializeSpaceError::StorageFailed("disk full".into()));
        assert_eq!(err.status.as_u16(), 500);
    }

    #[test]
    fn map_redeem_err_branches() {
        assert_eq!(
            map_redeem_err(RedeemPairingInvitationError::InvitationNotFound)
                .status
                .as_u16(),
            404
        );
        assert_eq!(
            map_redeem_err(RedeemPairingInvitationError::InvitationExpired)
                .status
                .as_u16(),
            404
        );
        assert_eq!(
            map_redeem_err(RedeemPairingInvitationError::PassphraseMismatch)
                .status
                .as_u16(),
            400
        );
        assert_eq!(
            map_redeem_err(RedeemPairingInvitationError::SponsorRejectedInvitation)
                .status
                .as_u16(),
            409
        );
        assert_eq!(
            map_redeem_err(RedeemPairingInvitationError::SponsorUnreachable)
                .status
                .as_u16(),
            503
        );
        assert_eq!(
            map_redeem_err(RedeemPairingInvitationError::Internal("boom".into()))
                .status
                .as_u16(),
            500
        );
    }
}
