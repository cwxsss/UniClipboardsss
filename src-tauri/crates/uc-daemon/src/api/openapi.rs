use utoipa::OpenApi;

use crate::api::dto::error::ApiErrorResponse;
use crate::api::dto::settings::{
    ContentTypesDto, FileSyncSettingsDto, GeneralSettingsDto, GetSettingsResponse,
    PairingSettingsDto, RetentionPolicyDto, RetentionRuleDto, RuleEvaluationDto,
    SecuritySettingsDto, SettingsDto, ShortcutKeyDto, SyncFrequencyDto, SyncSettingsDto, ThemeDto,
    UpdateChannelDto, UpdateSettingsResponse,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::settings::get_settings_handler,
        crate::api::settings::update_settings_handler,
    ),
    components(
        schemas(
            ContentTypesDto,
            ApiErrorResponse,
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
        )
    ),
    tags(
        (name = "settings", description = "Settings management APIs")
    )
)]
pub struct ApiDoc;
