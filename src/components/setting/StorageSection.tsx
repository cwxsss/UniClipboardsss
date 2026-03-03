import { useTranslation } from 'react-i18next'
import { SettingGroup } from './SettingGroup'

/**
 * Storage Section Placeholder
 *
 * 存储设置功能在新架构中尚未实现。
 * 后端 Settings 结构中不存在 storage 字段。
 * 此组件作为占位符，待后端实现后替换。
 *
 * 注意：后端有 RetentionPolicy 结构，可用于实现类似功能。
 */
const StorageSection: React.FC = () => {
  const { t } = useTranslation()

  return (
    <SettingGroup title={t('settings.categories.storage')}>
      <div className="text-center py-8 px-4">
        <p className="text-sm text-muted-foreground">
          {t('settings.sections.storage.placeholder') || 'Storage settings are not yet available.'}
        </p>
      </div>
    </SettingGroup>
  )
}

export default StorageSection
