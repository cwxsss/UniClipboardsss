import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import ClipboardItem from '@/components/clipboard/ClipboardItem'
import { invokeWithTrace } from '@/lib/tauri-command'

vi.mock('@/lib/tauri-command', () => ({
  invokeWithTrace: vi.fn(),
}))

// Mock daemon client blobUrl for image expansion
vi.mock('@/api/daemon/client', () => ({
  daemonClient: {
    blobUrl: vi.fn((path: string) => `http://127.0.0.1:12345${path}?auth=Session+test`),
  },
}))

const invokeMock = vi.mocked(invokeWithTrace)

describe('ClipboardItem', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    globalThis.fetch = vi.fn()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('expands by fetching resource bytes and decoding text', async () => {
    const preview = 'x'.repeat(260)
    const fullText = 'full content'
    const url = '/clipboard/blobs/blob-1'

    invokeMock.mockResolvedValue({
      blobId: 'blob-1',
      mimeType: 'text/plain',
      sizeBytes: fullText.length,
      url,
    })

    const fetchMock = vi.mocked(globalThis.fetch)
    fetchMock.mockResolvedValue({
      ok: true,
      arrayBuffer: async () => new TextEncoder().encode(fullText).buffer,
    } as Response)

    render(
      <ClipboardItem
        index={1}
        type="text"
        time="just now"
        content={{ display_text: preview, has_detail: true, size: fullText.length }}
        entryId="entry-1"
      />
    )

    await userEvent.click(screen.getByText(/Expand|展开/))

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('get_clipboard_entry_resource', {
        entryId: 'entry-1',
      })
      expect(fetchMock).toHaveBeenCalledWith(
        'http://127.0.0.1:12345/clipboard/blobs/blob-1?auth=Session+test'
      )
    })

    expect(await screen.findByText(fullText)).toBeInTheDocument()
  })

  it('expands image by loading resource url', async () => {
    const url = '/clipboard/blobs/image-1'
    const thumbnail = '/clipboard/thumbnails/image-1'

    invokeMock.mockResolvedValue({
      blobId: 'image-1',
      mimeType: 'image/png',
      sizeBytes: 123,
      url,
    })

    render(
      <ClipboardItem
        index={2}
        type="image"
        time="just now"
        content={{ thumbnail, size: 123, width: 10, height: 10 }}
        entryId="entry-2"
      />
    )

    const image = screen.getByAltText(/Clipboard Image|剪贴板图片/)
    expect(image).toHaveAttribute('src', thumbnail)

    await userEvent.click(screen.getByText(/Expand|展开/))

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('get_clipboard_entry_resource', {
        entryId: 'entry-2',
      })
    })

    expect(image).toHaveAttribute(
      'src',
      'http://127.0.0.1:12345/clipboard/blobs/image-1?auth=Session+test'
    )
  })
})
