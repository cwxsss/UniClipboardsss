import React, { useCallback, useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { contentTypeEntries, getDeviceIcon } from './device-utils'
import type { ContentTypes } from '@/api/daemon/member'
import { DEFAULT_SEND_CONTENT_TYPES } from '@/api/daemon/member'
import type { SpaceMember } from '@/api/daemon/members'
import { SettingRow } from '@/components/setting/SettingRow'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { ScrollArea } from '@/components/ui/scroll-area'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { formatPeerIdForDisplay } from '@/lib/utils'
import { useAppDispatch, useAppSelector } from '@/store/hooks'
import {
  fetchMemberSyncPreferences,
  updateMemberSyncPreferences,
} from '@/store/slices/devicesSlice'

interface DeviceSettingsSheetProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  deviceId: string
  device: SpaceMember | undefined
  globalAutoSyncOff: boolean
  globalFileSyncOff: boolean
  onUnpair: (peerId: string) => void
}

const DeviceSettingsSheet: React.FC<DeviceSettingsSheetProps> = ({
  open,
  onOpenChange,
  deviceId,
  device,
  globalAutoSyncOff,
  globalFileSyncOff,
  onUnpair,
}) => {
  const { t } = useTranslation()
  const dispatch = useAppDispatch()

  const preferences = useAppSelector(state => state.devices.memberSyncPreferences[deviceId])
  const isLoading = useAppSelector(
    state => state.devices.memberSyncPreferencesLoading[deviceId] ?? false
  )

  useEffect(() => {
    if (open && deviceId) {
      dispatch(fetchMemberSyncPreferences(deviceId))
    }
  }, [dispatch, deviceId, open])

  const handleSendEnabledToggle = useCallback(
    (checked: boolean) => {
      dispatch(
        updateMemberSyncPreferences({
          deviceId,
          patch: { sendEnabled: checked },
        })
      )
    },
    [dispatch, deviceId]
  )

  const handleSendContentTypeToggle = useCallback(
    (field: keyof ContentTypes, checked: boolean) => {
      dispatch(
        updateMemberSyncPreferences({
          deviceId,
          patch: { sendContentTypes: { [field]: checked } },
        })
      )
    },
    [dispatch, deviceId]
  )

  const handleRestoreDefaults = useCallback(async () => {
    // Phase 4b PR-3：UX 只露 send 列,所以 restore 仅重置 send 字段。
    // receive 字段保留服务端当前值（新 admit 的成员默认就是 true + 全开）。
    await dispatch(
      updateMemberSyncPreferences({
        deviceId,
        patch: {
          sendEnabled: true,
          sendContentTypes: DEFAULT_SEND_CONTENT_TYPES,
        },
      })
    )
    dispatch(fetchMemberSyncPreferences(deviceId))
  }, [dispatch, deviceId])

  const deviceName = device?.deviceName || t('devices.list.labels.unknownDevice')

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="flex flex-col">
        <SheetHeader>
          <div className="flex items-center gap-3">
            <div className="h-10 w-10 rounded-lg flex items-center justify-center ring-1 shadow-sm text-primary bg-primary/10 border-primary/20">
              {React.createElement(getDeviceIcon(device?.deviceName), { className: 'h-5 w-5' })}
            </div>
            <div className="min-w-0">
              <SheetTitle className="truncate">{deviceName}</SheetTitle>
              <SheetDescription className="flex items-center gap-2">
                <Badge variant={device?.connected ? 'default' : 'secondary'}>
                  {device?.connected
                    ? t('devices.list.status.online')
                    : t('devices.list.status.offline')}
                </Badge>
                <span className="font-mono text-xs">{formatPeerIdForDisplay(device?.peerId)}</span>
              </SheetDescription>
            </div>
          </div>
        </SheetHeader>

        <ScrollArea className="flex-1 min-h-0 -mx-4">
          <div className="px-4 py-1">
            {isLoading && !preferences ? (
              <div className="space-y-3">
                {[1, 2, 3, 4].map(i => (
                  <div key={i} className="flex items-center justify-between py-3 px-1">
                    <div className="space-y-1.5">
                      <Skeleton className="h-4 w-32" />
                      <Skeleton className="h-3 w-48" />
                    </div>
                    <Skeleton className="w-6 h-3.5 rounded-full" />
                  </div>
                ))}
              </div>
            ) : (
              <div className="space-y-1.5">
                <h3 className="text-xs font-medium text-muted-foreground px-1 uppercase tracking-wider">
                  {t('devices.settings.sync.title')}
                </h3>
                <div className="divide-y divide-border/40">
                  {/* Send enabled toggle */}
                  <SettingRow
                    label={t('devices.settings.sync.rules.sendEnabled.title')}
                    description={t('devices.settings.sync.rules.sendEnabled.description')}
                  >
                    <Switch
                      size="sm"
                      checked={preferences?.sendEnabled ?? true}
                      onCheckedChange={handleSendEnabledToggle}
                      disabled={globalAutoSyncOff || isLoading}
                    />
                  </SettingRow>

                  {/* Send content type toggles */}
                  {contentTypeEntries.map(({ field, i18nKey, status }) => {
                    const isComingSoon = status === 'coming_soon'
                    const isSendDisabled = !preferences?.sendEnabled
                    const isGlobalFileSyncDisabled = field === 'file' && globalFileSyncOff
                    const isDisabled =
                      isComingSoon ||
                      isSendDisabled ||
                      globalAutoSyncOff ||
                      isGlobalFileSyncDisabled ||
                      isLoading

                    let labelExtra: React.ReactNode = null
                    if (isComingSoon) {
                      labelExtra = (
                        <Badge variant="secondary">{t('devices.settings.badges.comingSoon')}</Badge>
                      )
                    } else if (isGlobalFileSyncDisabled) {
                      labelExtra = (
                        <Badge
                          variant="outline"
                          className="border-amber-500/20 bg-amber-500/10 text-amber-600 dark:text-amber-400"
                        >
                          {t('devices.settings.badges.globalFileSyncOff')}
                        </Badge>
                      )
                    }

                    return (
                      <SettingRow
                        key={field}
                        label={t(`devices.settings.sync.rules.${i18nKey}.title`)}
                        labelExtra={labelExtra}
                        description={t(`devices.settings.sync.rules.${i18nKey}.description`)}
                      >
                        <Switch
                          size="sm"
                          checked={preferences?.sendContentTypes[field] ?? true}
                          onCheckedChange={checked => handleSendContentTypeToggle(field, checked)}
                          disabled={isDisabled}
                        />
                      </SettingRow>
                    )
                  })}
                </div>
              </div>
            )}
          </div>
        </ScrollArea>

        <SheetFooter className="flex-row border-t">
          <Button
            variant="outline"
            size="sm"
            className="flex-1 min-w-0"
            onClick={handleRestoreDefaults}
            disabled={globalAutoSyncOff || isLoading}
          >
            <span className="truncate">{t('devices.settings.sync.restoreDefaults')}</span>
          </Button>
          <Button
            variant="destructive"
            size="sm"
            className="flex-1 min-w-0"
            onClick={() => onUnpair(deviceId)}
          >
            <span className="truncate">{t('devices.list.actions.unpair')}</span>
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  )
}

export default DeviceSettingsSheet
