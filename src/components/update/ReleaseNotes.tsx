import { useTranslation } from 'react-i18next'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

interface ReleaseNotesProps {
  content: string
  fallback: string
}

const ZH_SEPARATOR = '<!-- zh -->'

export function ReleaseNotes({ content, fallback }: ReleaseNotesProps) {
  const { i18n } = useTranslation()
  const body = content?.trim()
  if (!body) return <span className="text-muted-foreground">{fallback}</span>

  let displayContent = body
  if (body.includes(ZH_SEPARATOR)) {
    const [enPart, zhPart] = body.split(ZH_SEPARATOR, 2)
    const zhContent = zhPart?.trim()
    if (i18n.language.startsWith('zh') && zhContent) {
      displayContent = zhContent
    } else {
      displayContent = enPart.trim()
    }
  }

  return (
    <div
      className="prose prose-sm dark:prose-invert max-w-none
                    prose-headings:text-sm prose-headings:font-semibold prose-headings:mt-3 prose-headings:mb-1
                    prose-ul:my-1 prose-li:my-0 prose-p:my-1"
    >
      <Markdown remarkPlugins={[remarkGfm]}>{displayContent}</Markdown>
    </div>
  )
}
