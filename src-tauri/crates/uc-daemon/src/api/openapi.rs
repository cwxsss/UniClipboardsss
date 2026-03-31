//! OpenAPI security definitions for the daemon HTTP API.
//!
//! L2 endpoints require a valid session JWT. The client obtains a session token
//! by calling `POST /auth/dev-token` (dev only) or through the pairing flow.
//! The token is passed via the `Authorization` header as `Session <token>`.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::api::dto::device::{GetLocalDeviceInfoResponse, LocalDeviceInfoDto};
use crate::api::dto::encryption::{EncryptionStateResponse, KeychainAccessResponse};
use crate::api::dto::error::ApiErrorResponse;
use crate::api::dto::settings::{
    ContentTypesDto, FileSyncSettingsDto, GeneralSettingsDto, GetSettingsResponse,
    PairingSettingsDto, RetentionPolicyDto, RetentionRuleDto, RuleEvaluationDto,
    SecuritySettingsDto, SettingsDto, ShortcutKeyDto, SyncFrequencyDto, SyncSettingsDto, ThemeDto,
    UpdateChannelDto, UpdateSettingsResponse,
};
use crate::api::dto::setup::{
    GetSetupStateResponse, SetupActionResponse, SetupResetResponse, SetupSelectPeerRequest,
    SetupStateResponseDto, SetupSubmitPassphraseRequest,
};

/// Adds a `session` Bearer security scheme to the OpenAPI document and applies it
/// to all L2+ paths.
///
/// The scheme uses the `Authorization` header with a `Session <token>` value.
/// This matches the middleware in `security/middleware.rs` which validates JWT
/// session tokens after stripping the `Session ` prefix.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);

        components.add_security_scheme(
            "session",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("Authorization"))),
        );

        // Batch-apply the security scheme to ALL paths.
        // L1 paths (health, dev-token) are in a separate OpenApi derivation in dev.rs.
        for (_, path_item) in openapi.paths.paths.iter_mut() {
            for op in path_item.operations.values_mut() {
                op.security.get_or_insert_with(Default::default).push(
                    utoipa::openapi::security::SecurityRequirement::new(
                        "session",
                        std::iter::empty::<String>(),
                    ),
                );
            }
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    paths(
        crate::api::settings::get_settings_handler,
        crate::api::settings::update_settings_handler,
        crate::api::encryption::get_encryption_state_handler,
        crate::api::encryption::unlock_handler,
        crate::api::encryption::lock_handler,
        crate::api::encryption::verify_keychain_access_handler,
        crate::api::device::get_local_device_info_handler,
        crate::api::setup::get_setup_state,
        crate::api::setup::start_host,
        crate::api::setup::start_join,
        crate::api::setup::select_peer,
        crate::api::setup::confirm_peer,
        crate::api::setup::submit_passphrase,
        crate::api::setup::cancel,
        crate::api::setup::reset,
    ),
    components(
        schemas(
            ContentTypesDto,
            ApiErrorResponse,
            GetLocalDeviceInfoResponse,
            LocalDeviceInfoDto,
            GetSettingsResponse,
            UpdateSettingsResponse,
            SettingsDto,
            GeneralSettingsDto,
            SyncSettingsDto,
            SyncFrequencyDto,
            RetentionPolicyDto,
            RetentionRuleDto,
            RuleEvaluationDto,
            SecuritySettingsDto,
            PairingSettingsDto,
            FileSyncSettingsDto,
            ShortcutKeyDto,
            ThemeDto,
            UpdateChannelDto,
            EncryptionStateResponse,
            KeychainAccessResponse,
            GetSetupStateResponse,
            SetupActionResponse,
            SetupResetResponse,
            SetupStateResponseDto,
            SetupSelectPeerRequest,
            SetupSubmitPassphraseRequest,
        )
    ),
    tags(
        (name = "device", description = "Local device identity"),
        (name = "settings", description = "Settings management APIs"),
        (name = "encryption", description = "Encryption state and session management"),
        (name = "setup", description = "Device setup and pairing flow"),
    )
)]
pub struct ApiDoc;
