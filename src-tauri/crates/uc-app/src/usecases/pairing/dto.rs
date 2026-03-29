//! Pairing aggregation DTOs for command layer consumption
//! 配对聚合 DTO，供命令层消费

use serde::{Deserialize, Serialize};

/// Peer information combining discovery and pairing state.
/// 结合发现和配对状态的对等端信息。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct P2PPeerInfo {
    pub peer_id: String,
    pub device_name: Option<String>,
    pub addresses: Vec<String>,
    pub is_paired: bool,
    pub connected: bool,
}

/// Paired device information for frontend display.
/// 用于前端显示的已配对设备信息。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedPeer {
    pub peer_id: String,
    pub device_name: String,
    pub shared_secret: Vec<u8>,
    pub paired_at: String,
    pub last_seen: Option<String>,
    pub last_known_addresses: Vec<String>,
    pub connected: bool,
}
