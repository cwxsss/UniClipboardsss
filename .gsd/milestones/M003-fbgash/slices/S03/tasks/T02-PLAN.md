---
estimated_steps: 21
estimated_files: 1
skills_used: []
---

# T02: React hooks for daemon WS events

Create `src/hooks/useDaemonEvents.ts`:

```typescript
// Subscribe to clipboard.new-content events
export function useClipboardNewContent(
  callback: (entry: ClipboardEntryDto) => void
): void {
  // useEffect: daemonWs.subscribe(['clipboard'], handler), return unsubscribe on cleanup
}

// Subscribe to pairing events
export function usePairingEvents(callbacks: {
  onVerification?: (data: PairingVerificationData) => void
  onComplete?: (data: PairingCompleteData) => void
  onFailed?: (data: PairingFailedData) => void
}): void

// Subscribe to encryption state events
export function useEncryptionState(
  onReady: () => void,
  onFailed: () => void
): void
```

All hooks: call daemonWs.subscribe() in useEffect, unsubscribe on cleanup. Handle reconnection gracefully (hooks re-subscribe automatically since daemonWs maintains subscriptions).

## Inputs

- `src/lib/daemon-ws.ts`
- `src/hooks/ (existing hook patterns)`

## Expected Output

- `src/hooks/useDaemonEvents.ts`

## Verification

TypeScript compiles. Hooks correctly subscribe/unsubscribe on mount/unmount. Multiple concurrent subscriptions work.
