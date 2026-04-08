---
estimated_steps: 25
estimated_files: 1
skills_used: []
---

# T01: DaemonWsClient class with reconnect logic

Create `src/lib/daemon-ws.ts`:

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

## Inputs

- `src-tauri/crates/uc-daemon/src/api/ws_handler.rs (topic system)`
- `src/api/daemon/client.ts (session token)`

## Expected Output

- `src/lib/daemon-ws.ts`

## Verification

Unit tests: connect succeeds, subscribe returns unsubscribe fn, reconnect logic works. Browser test against running daemon WS.
