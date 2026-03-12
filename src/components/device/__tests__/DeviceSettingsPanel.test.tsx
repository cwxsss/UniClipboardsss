import { configureStore } from '@reduxjs/toolkit'
import { render, screen } from '@testing-library/react'
import { Provider } from 'react-redux'
import { describe, it, expect, vi } from 'vitest'
import DeviceSettingsPanel from '../DeviceSettingsPanel'
import i18n from '@/i18n'
import devicesReducer from '@/store/slices/devicesSlice'

vi.mock('framer-motion', () => ({
  motion: {
    div: ({ children, className, ...props }: any) => (
      <div className={className} {...props}>
        {children}
      </div>
    ),
  },
  AnimatePresence: ({ children }: any) => <>{children}</>,
}))

// Mock the Tauri invoke so thunks don't actually call the backend
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue(null),
}))

const DEVICE_ID = 'test-device-id'

const defaultContentTypes = {
  text: true,
  image: true,
  link: true,
  file: true,
  code_snippet: true,
  rich_text: true,
}

function createDevicesState(overrides: Record<string, unknown> = {}) {
  return {
    localDevice: null,
    localDeviceLoading: false,
    localDeviceError: null,
    pairedDevices: [] as never[],
    pairedDevicesLoading: false,
    pairedDevicesError: null,
    deviceSyncSettings: {
      [DEVICE_ID]: {
        auto_sync: true,
        sync_frequency: 'realtime' as const,
        content_types: { ...defaultContentTypes },
        max_file_size_mb: 100,
      },
    },
    deviceSyncSettingsLoading: {} as Record<string, boolean>,
    ...overrides,
  }
}

function createStore(overrides: Record<string, unknown> = {}) {
  const devicesState = createDevicesState(overrides)
  return configureStore({
    reducer: { devices: devicesReducer },
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    preloadedState: { devices: devicesState as any },
  })
}

function renderWithStore(overrides: Record<string, any> = {}) {
  const store = createStore(overrides)
  return render(
    <Provider store={store}>
      <DeviceSettingsPanel deviceId={DEVICE_ID} deviceName="Test Device" />
    </Provider>
  )
}

describe('DeviceSettingsPanel', () => {
  it('renders sync rules section', () => {
    renderWithStore()
    expect(screen.getByText(i18n.t('devices.settings.sync.title'))).toBeInTheDocument()
    expect(
      screen.getByText(i18n.t('devices.settings.sync.rules.autoSync.title'))
    ).toBeInTheDocument()
    expect(
      screen.getByText(i18n.t('devices.settings.sync.rules.syncText.title'))
    ).toBeInTheDocument()
    // Coming Soon badges should appear on non-editable types
    const comingSoonBadges = screen.getAllByText(i18n.t('devices.settings.badges.comingSoon'))
    expect(comingSoonBadges.length).toBe(4) // file, link, code_snippet, rich_text
  })

  it('text and image toggles show no badge when auto_sync is on', () => {
    renderWithStore()
    const textRow = screen
      .getByText(i18n.t('devices.settings.sync.rules.syncText.title'))
      .closest('div.flex.items-center.gap-2')!
    const imageRow = screen
      .getByText(i18n.t('devices.settings.sync.rules.syncImage.title'))
      .closest('div.flex.items-center.gap-2')!

    expect(textRow.querySelector('span')).toBeNull()
    expect(imageRow.querySelector('span')).toBeNull()
  })

  it('coming soon types show Coming Soon badge', () => {
    renderWithStore()
    const comingSoonLabels = [
      'devices.settings.sync.rules.syncFile.title',
      'devices.settings.sync.rules.syncLink.title',
      'devices.settings.sync.rules.syncCodeSnippet.title',
      'devices.settings.sync.rules.syncRichText.title',
    ]
    for (const label of comingSoonLabels) {
      const row = screen.getByText(i18n.t(label)).closest('div.flex.items-center.gap-2')!
      expect(row.querySelector('span')).not.toBeNull()
      expect(row.querySelector('span')!.textContent).toBe(
        i18n.t('devices.settings.badges.comingSoon')
      )
    }
  })

  it('shows all-disabled warning when all content types are false and auto_sync is on', () => {
    renderWithStore({
      deviceSyncSettings: {
        [DEVICE_ID]: {
          auto_sync: true,
          sync_frequency: 'realtime',
          content_types: {
            text: false,
            image: false,
            link: false,
            file: false,
            code_snippet: false,
            rich_text: false,
          },
          max_file_size_mb: 100,
        },
      },
    })
    expect(
      screen.getByText(i18n.t('devices.settings.sync.allContentTypesDisabled'))
    ).toBeInTheDocument()
  })

  it('hides all-disabled warning when auto_sync is off', () => {
    renderWithStore({
      deviceSyncSettings: {
        [DEVICE_ID]: {
          auto_sync: false,
          sync_frequency: 'realtime',
          content_types: {
            text: false,
            image: false,
            link: false,
            file: false,
            code_snippet: false,
            rich_text: false,
          },
          max_file_size_mb: 100,
        },
      },
    })
    expect(
      screen.queryByText(i18n.t('devices.settings.sync.allContentTypesDisabled'))
    ).not.toBeInTheDocument()
  })
})
