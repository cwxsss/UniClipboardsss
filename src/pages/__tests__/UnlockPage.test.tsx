import { render, screen, act } from '@testing-library/react'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { unlockEncryptionSession, verifyKeychainAccess } from '@/api/security'
import i18n from '@/i18n'
import UnlockPage from '@/pages/UnlockPage'

vi.mock('@/api/security', () => ({
  unlockEncryptionSession: vi.fn(),
  verifyKeychainAccess: vi.fn(),
}))

vi.mock('@/hooks/usePlatform', () => ({
  usePlatform: () => ({ isMac: false }),
}))

const updateSecuritySettingMock = vi.fn()

vi.mock('@/hooks/useSetting', () => ({
  useSetting: () => ({
    setting: { security: { autoUnlockEnabled: false } },
    updateSecuritySetting: updateSecuritySettingMock,
    loading: false,
  }),
}))

describe('UnlockPage', () => {
  beforeEach(async () => {
    vi.clearAllMocks()
    await i18n.changeLanguage('zh-CN')
    vi.mocked(verifyKeychainAccess).mockResolvedValue(true)
  })

  it('notifies parent immediately when unlock succeeds', async () => {
    const onUnlockSucceeded = vi.fn()
    vi.mocked(unlockEncryptionSession).mockResolvedValue(true)

    render(<UnlockPage onUnlockSucceeded={onUnlockSucceeded} />)

    await act(async () => {
      screen.getByRole('button', { name: i18n.t('unlock.button') }).click()
    })

    expect(unlockEncryptionSession).toHaveBeenCalledTimes(1)
    expect(onUnlockSucceeded).toHaveBeenCalledTimes(1)
  })
})
