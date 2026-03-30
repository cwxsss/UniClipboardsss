# S03: Frontend WebSocket Direct Connection & Event Migration

**Goal:** Create DaemonWsClient for direct WebSocket connection. Migrate clipboard/encryption/lifecycle event listeners from Tauri listen() to daemon WS. Add React hooks.
**Demo:** After this: Frontend connects to daemon WS directly; clipboard new-content events trigger UI update without Tauri emit

## Tasks
- [x] **T01: Created DaemonWsClient with WebSocket connect/subscribe/reconnect and exponential backoff; TypeScript clean; unit tests written but blocked by bun test** — Create `src/lib/daemon-ws.ts`:

```typescript
interface DaemonWsEvent<T = unknown> {
  topic: string;
  type: string;
  ts: number;
  sessionId: string;
  payload: T;
}

type WsEventCallback<T = unknown> = (event: DaemonWsEvent<T>) => void;

class DaemonWsClient {
  connect(wsUrl: string): Promise<void>
  disconnect(): void
  subscribe<T>(topics: string[], callback: WsEventCallback<T>): () => void
  // Returns unsubscribe function. Internally manages WS connection and reconnection.
}

export const daemonWs: DaemonWsClient
```

Connection flow:
1. connect() opens WebSocket to wsUrl (from DaemonConfig)
2. First message: send session token or authenticate
3. subscribe() sends: `{ action: "subscribe", topics: [...], nonce: "random" }`
4. Incoming messages: parse DaemonWsEvent, dispatch to matching topic callbacks
5. Reconnect: exponential backoff 1s→30s max, 10 attempts max, then give up
6. On reconnect: re-subscribe to all active topics
  - Estimate: medium
  - Files: src/lib/daemon-ws.ts
  - Verify: Unit tests: connect succeeds, subscribe returns unsubscribe fn, reconnect logic works. Browser test against running daemon WS.
- [x] **T02: Created useClipboardNewContent, usePairingEvents, and useEncryptionState React hooks wrapping daemon WS subscribe; TypeScript clean; 20 vitest tests pass** — Create `src/hooks/useDaemonEvents.ts`:

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
  - Estimate: medium
  - Files: src/hooks/useDaemonEvents.ts
  - Verify: TypeScript compiles. Hooks correctly subscribe/unsubscribe on mount/unmount. Multiple concurrent subscriptions work.
- [ ] **T03: Migrate Tauri listen() calls to daemonWs.subscribe()** — Find all existing `listen()` calls for clipboard, pairing, encryption, lifecycle events in src/ and replace with daemonWs.subscribe() equivalents.

Patterns to replace:
- `listen('daemon://realtime', ...)` → `daemonWs.subscribe(['clipboard', 'peers', 'pairing', ...], ...)`
- `listen('clipboard://event', ...)` → `daemonWs.subscribe(['clipboard'], ...)`
- `listen('daemon://ws-reconnected', ...)` → internal reconnect handler
- `listen('encryption://ready', ...)` → `useEncryptionState(onReady, onFailed)` or equivalent

Keep DaemonWsBridge in uc-tauri alive for now (other consumers may depend on it). Frontend just bypasses it for its own subscriptions.
  - Estimate: medium
  - Files: src/api/realtime.ts, src/store/ slices that use realtime events, src/App.tsx or equivalent root component
  - Verify: Browser test: copy on device A → WS event received by device B within 100ms → UI updates. Kill daemon → restart → frontend auto-reconnects and resubscribes.
