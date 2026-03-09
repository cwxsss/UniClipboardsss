import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'

interface ReleaseNotesProps {
  content: string
  fallback: string
}

export function ReleaseNotes({ content, fallback }: ReleaseNotesProps) {
  const body = content?.trim()
  if (!body) return <span className="text-muted-foreground">{fallback}</span>

  return (
    <div
      className="prose prose-sm dark:prose-invert max-w-none
                    prose-headings:text-sm prose-headings:font-semibold prose-headings:mt-3 prose-headings:mb-1
                    prose-ul:my-1 prose-li:my-0 prose-p:my-1"
    >
      <Markdown remarkPlugins={[remarkGfm]}>{body}</Markdown>
    </div>
  )
}
