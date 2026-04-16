use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralSettings {
    pub auto_start: bool,
    pub silent_start: bool,
    pub auto_check_update: bool,
    pub theme: Theme,
    pub theme_color: Option<String>,
    pub language: Option<String>,
    pub device_name: Option<String>,
    /// Update channel preference. `None` means auto-detect from version string;
    /// `Some(channel)` means the user has overridden the channel.
    #[serde(default)]
    pub update_channel: Option<UpdateChannel>,
    /// Whether anonymous diagnostic telemetry is enabled.
    /// When `true` and an OTLP endpoint is configured, the app sends
    /// info/warn/error level events (never clipboard content).
    #[serde(default = "default_telemetry_enabled")]
    pub telemetry_enabled: bool,
}

fn default_telemetry_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    Stable,
    Alpha,
    Beta,
    Rc,
}

/// A keyboard shortcut value that can be either a single key combo or multiple alternatives.
///
/// Serialised with `#[serde(untagged)]` so that `"Ctrl+C"` and `["Ctrl+C","Meta+C"]` are both
/// accepted without a wrapping tag, matching the TypeScript type `string | string[]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ShortcutKey {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContentTypes {
    pub text: bool,
    pub image: bool,
    pub link: bool,
    pub file: bool,
    pub code_snippet: bool,
    pub rich_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncSettings {
    pub auto_sync: bool,
    pub sync_frequency: SyncFrequency,

    #[serde(default)]
    pub content_types: ContentTypes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncFrequency {
    Realtime,
    Interval,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionRule {
    /// 按时间清理
    ByAge {
        #[serde_as(as = "DurationSeconds<u64>")]
        max_age: Duration,
    },

    /// 按总数量上限
    ByCount { max_items: usize },

    /// 按内容类型的最大存活时间
    ByContentType {
        content_type: ContentTypes,
        #[serde_as(as = "DurationSeconds<u64>")]
        max_age: Duration,
    },

    /// 按磁盘占用大小
    ByTotalSize { max_bytes: u64 },

    /// 敏感内容快速过期
    Sensitive {
        #[serde_as(as = "DurationSeconds<u64>")]
        max_age: Duration,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleEvaluation {
    AnyMatch, // OR（推荐，默认）
    AllMatch, // AND（极少用）
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RetentionPolicy {
    pub enabled: bool,
    pub rules: Vec<RetentionRule>,
    pub skip_pinned: bool,
    pub evaluation: RuleEvaluation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecuritySettings {
    /// 是否启用本地数据加密
    pub encryption_enabled: bool,

    /// 是否已经在 keyring 中设置过口令
    ///
    /// 仅用于 UI 与流程判断
    /// 不代表当前口令是否“可用”
    pub passphrase_configured: bool,

    /// 是否启用启动时自动解锁
    ///
    /// 仅用于 UI 与流程判断
    /// 需要用户在系统弹窗中选择“始终允许”才能静默生效
    #[serde(default)]
    pub auto_unlock_enabled: bool,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingSettings {
    #[serde_as(as = "DurationSeconds<u64>")]
    pub step_timeout: Duration,
    #[serde_as(as = "DurationSeconds<u64>")]
    pub user_verification_timeout: Duration,
    #[serde_as(as = "DurationSeconds<u64>")]
    pub session_timeout: Duration,
    pub max_retries: u8,
    pub protocol_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileSyncSettings {
    pub file_sync_enabled: bool,
    pub small_file_threshold: u64,
    pub max_file_size: u64,
    pub file_cache_quota_per_device: u64,
    pub file_retention_hours: u32,
    pub file_auto_cleanup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "current_schema_version")]
    pub schema_version: u32,

    #[serde(default)]
    pub general: GeneralSettings,

    #[serde(default)]
    pub sync: SyncSettings,

    #[serde(default)]
    pub retention_policy: RetentionPolicy,

    #[serde(default)]
    pub security: SecuritySettings,

    #[serde(default)]
    pub pairing: PairingSettings,

    #[serde(default)]
    pub keyboard_shortcuts: HashMap<String, ShortcutKey>,

    #[serde(default)]
    pub file_sync: FileSyncSettings,
    // #[serde(default)]
    // pub network: NetworkSettings,
}

/// The current schema version used for settings persistence.
///
/// # Returns
///
/// The schema version as a `u32`.
///
/// # Examples
///
/// ```
/// use uc_core::settings::model::{current_schema_version, CURRENT_SCHEMA_VERSION};
///
/// let v = current_schema_version();
/// assert_eq!(v, CURRENT_SCHEMA_VERSION);
/// ```
pub fn current_schema_version() -> u32 {
    CURRENT_SCHEMA_VERSION
}
