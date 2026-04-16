use crate::settings::model::SyncSettings;
use crate::PeerId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PairingState {
    Pending,
    Trusted,
    Revoked,
}

impl fmt::Display for PairingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            PairingState::Pending => "Pending",
            PairingState::Trusted => "Trusted",
            PairingState::Revoked => "Revoked",
        };
        f.write_str(label)
    }
}

impl FromStr for PairingState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Pending" => Ok(PairingState::Pending),
            "Trusted" => Ok(PairingState::Trusted),
            "Revoked" => Ok(PairingState::Revoked),
            _ => Err(format!("invalid PairingState: {s}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub peer_id: PeerId,
    pub pairing_state: PairingState,
    pub identity_fingerprint: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub device_name: String,
    #[serde(default)]
    pub sync_settings: Option<SyncSettings>,
}

/// Returns the effective sync settings for a device.
///
/// If the device has per-device overrides, those are used; otherwise the
/// global defaults are returned.
pub fn resolve_sync_settings<'a>(
    device: &'a PairedDevice,
    global: &'a SyncSettings,
) -> &'a SyncSettings {
    device.sync_settings.as_ref().unwrap_or(global)
}
