//! Shared peer-snapshot derivation used by both the WS `peers.changed`
//! emitter (`presence_monitor`) and the read-only HTTP/WS query path
//! (`DaemonQueryService::peers`).
//!
//! Source of truth post-libp2p:
//! * `MemberRepositoryPort.list()` 是 membership 真相;
//! * `PresencePort.current_state(device_id)` 给出每个远端成员的 online/offline;
//! * `DeviceIdentityPort.current_device_id()` 用于排除本机。
//!
//! 旧的 libp2p `PeerDirectoryPort` 的 discovered/connected 列表已退役
//! (5b 起 DisabledNetwork 桩),不再参与计算。
//!
//! `addresses` 留空——iroh stack 不再以 multiaddr 形式向上层暴露;
//! `pairing_state` 固定 `"Trusted"`——进入 `space_member` 即等同已配对完成。

use std::sync::Arc;

use anyhow::Result;

use uc_app::runtime::CoreRuntime;
use uc_core::ports::presence::ReachabilityState;
use uc_core::ports::PresencePort;
use uc_daemon_contract::api::types::PeerSnapshotDto;

pub async fn derive_peer_snapshot(
    presence: &Arc<dyn PresencePort>,
    runtime: &CoreRuntime,
) -> Result<Vec<PeerSnapshotDto>> {
    let deps = runtime.wiring_deps();
    let local_id = deps.device.device_identity.current_device_id();
    let members = deps
        .device
        .member_repo
        .list()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list space members: {e}"))?;

    let mut snapshots = Vec::with_capacity(members.len());
    for member in members {
        if member.device_id == local_id {
            continue;
        }
        let state = presence.current_state(&member.device_id).await;
        let device_name = if member.device_name.is_empty() {
            None
        } else {
            Some(member.device_name.clone())
        };
        snapshots.push(PeerSnapshotDto {
            peer_id: member.device_id.as_str().to_string(),
            device_name,
            addresses: Vec::new(),
            is_paired: true,
            connected: matches!(state, ReachabilityState::Online),
            pairing_state: "Trusted".to_string(),
        });
    }
    Ok(snapshots)
}
