/**
 * Members API — typed accessors for daemon space-member endpoints.
 *
 * Replaces the libp2p-era pairing.ts wrappers (Slice 4 P5a-2). The
 * backend endpoints are unchanged; only the frontend module name and
 * type shape are. SpaceMember is now aligned with the daemon
 * `SpaceMemberDto` and no longer carries the mDNS-era
 * `sharedSecret`/`pairedAt`/`lastSeen`/`lastKnownAddresses` fields.
 */

import { daemonClient } from './client'

export interface LocalDeviceInfo {
  peerId: string
  deviceName: string
}

interface LocalDeviceInfoResponse {
  data: LocalDeviceInfo
  ts: number
}

/**
 * Space member — matches `SpaceMemberDto` on the Rust side.
 *
 * `connected` is the runtime presence flag. After the libp2p adapter
 * was retired in Slice 4 P5a-1 it is hard-coded to `false` until the
 * iroh-side presence channel is wired in.
 */
export interface SpaceMember {
  peerId: string
  deviceName: string
  pairingState: string
  lastSeenAtMs: number | null
  connected: boolean
}

/**
 * Get the local device's peer id + resolved device name.
 *
 * 获取本地设备信息（peer ID + 解析后的设备名称）。
 */
export async function getLocalDeviceInfo(): Promise<LocalDeviceInfo> {
  const response = await daemonClient.request<LocalDeviceInfoResponse>('/device/me')
  return response.data
}

/**
 * Get the list of admitted space members.
 *
 * 获取已配对的设备列表。
 */
export async function getPairedPeers(): Promise<SpaceMember[]> {
  return daemonClient.request<SpaceMember[]>('/paired-devices')
}

/**
 * Alias of {@link getPairedPeers} kept for source-level compatibility
 * with consumers that previously distinguished "with status".
 */
export async function getPairedPeersWithStatus(): Promise<SpaceMember[]> {
  return getPairedPeers()
}

/**
 * Remove a paired device from the local member registry.
 *
 * 取消配对：从本机成员仓库移除该设备。
 */
export async function unpairDevice(peerId: string): Promise<void> {
  await daemonClient.request<void>('/pairing/unpair', {
    method: 'POST',
    body: { peerId },
  })
}
