---
estimated_steps: 1
estimated_files: 8
skills_used: []
---

# T02: Migrate remaining clipboard UI and preview consumers to daemon transport

Replace the live clipboard business callers that still import runtime functions from `@/api/clipboardItems`. Move list hydration, single-entry reloads, preview/detail/resource loading, and clear/stats consumers onto daemon clients/store thunks, while leaving `clipboardItems.ts` as a types-and-native-utility module only.

## Inputs

- ``src/hooks/useClipboardCollection.ts``
- ``src/hooks/useClipboardEventStream.ts``
- ``src/components/clipboard/ClipboardPreview.tsx``
- ``src/components/clipboard/ClipboardItem.tsx``
- ``src/preview-panel/PreviewPanel.tsx``
- ``src/components/layout/ActionBar.tsx``
- ``src/api/clipboardItems.ts``
- ``src/api/daemon/clipboard.ts``

## Expected Output

- ``src/hooks/useClipboardCollection.ts``
- ``src/hooks/useClipboardEventStream.ts``
- ``src/components/clipboard/ClipboardPreview.tsx``
- ``src/components/clipboard/ClipboardItem.tsx``
- ``src/preview-panel/PreviewPanel.tsx``
- ``src/components/layout/ActionBar.tsx``
- ``src/hooks/__tests__/useClipboardEventStream.test.tsx``
- ``src/preview-panel/__tests__/PreviewPanel.test.tsx``

## Verification

npx vitest run src/hooks/__tests__/useClipboardEventStream.test.tsx src/preview-panel/__tests__/PreviewPanel.test.tsx src/api/daemon/__tests__/clipboard.test.ts

## Observability Impact

Preview and item-loading tests must verify error-state handling so daemon detail/resource failures do not leave stale or silently empty UI.
