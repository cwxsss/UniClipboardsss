---
estimated_steps: 7
estimated_files: 3
skills_used: []
---

# T03: Migrate Tauri listen() calls to daemonWs.subscribe()

Find all existing `listen()` calls for clipboard, pairing, encryption, lifecycle events in src/ and replace with daemonWs.subscribe() equivalents.

Patterns to replace:
- `listen('daemon://realtime', ...)` → `daemonWs.subscribe(['clipboard', 'peers', 'pairing', ...], ...)`
- `listen('clipboard://event', ...)` → `daemonWs.subscribe(['clipboard'], ...)`
- `listen('daemon://ws-reconnected', ...)` → internal reconnect handler
- `listen('encryption://ready', ...)` → `useEncryptionState(onReady, onFailed)` or equivalent

Keep DaemonWsBridge in uc-tauri alive for now (other consumers may depend on it). Frontend just bypasses it for its own subscriptions.

## Inputs

- `src/api/realtime.ts (current event system)`
- `src/api/daemon/client.ts (connection setup)`

## Expected Output

- `Updated src/ files with WS event migration`

## Verification

Browser test: copy on device A → WS event received by device B within 100ms → UI updates. Kill daemon → restart → frontend auto-reconnects and resubscribes.
