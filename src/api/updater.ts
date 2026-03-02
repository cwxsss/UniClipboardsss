import { invokeWithTrace } from '@/lib/tauri-command'
import type { UpdateChannel } from '@/types/setting'

export interface UpdateMetadata {
  version: string
  currentVersion: string
  body?: string
  date?: string
}

/**
 * 检查更新
 * @param channel 可选的更新频道，null 表示自动检测
 * @returns Promise，返回更新信息或 null（无更新）
 */
export async function checkForUpdate(
  channel?: UpdateChannel | null
): Promise<UpdateMetadata | null> {
  try {
    return await invokeWithTrace('check_for_update', { channel: channel ?? null })
  } catch (error) {
    console.error('检查更新失败:', error)
    throw error
  }
}

/**
 * 安装更新
 * @returns Promise，安装完成后应用重启
 */
export async function installUpdate(): Promise<void> {
  try {
    return await invokeWithTrace('install_update')
  } catch (error) {
    console.error('安装更新失败:', error)
    throw error
  }
}
