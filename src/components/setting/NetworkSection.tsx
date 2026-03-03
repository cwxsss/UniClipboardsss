import { useTranslation } from 'react-i18next'
import { SettingGroup } from './SettingGroup'

/**
 * Network Section Placeholder
 *
 * 网络设置功能在新架构中尚未实现。
 * 后端 Settings 结构中不存在 network 字段。
 * 此组件作为占位符，待后端实现后替换。
 */
const NetworkSection: React.FC = () => {
  const { t } = useTranslation()

  return (
    <SettingGroup title={t('settings.categories.network')}>
      <div className="text-center py-8 px-4">
        <p className="text-sm text-muted-foreground">
          {t('settings.sections.network.placeholder') || 'Network settings are not yet available.'}
        </p>
      </div>
    </SettingGroup>
  )
}

export default NetworkSection
