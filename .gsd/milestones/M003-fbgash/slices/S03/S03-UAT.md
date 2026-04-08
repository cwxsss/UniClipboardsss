# S03: Frontend WebSocket Direct Connection & Event Migration — UAT

**Milestone:** M003-fbgash
**Written:** 2026-03-30T05:45:35.067Z

# S03-UAT: Frontend WebSocket Direct Connection & Event Migration

## Preconditions
- Dev environment: `bun tauri:dev` running with daemon process
- App is at the main dashboard (past any setup flow)
- DevTools console open to observe WS connection and event logs

## Test Cases

### TC-WS-01: WebSocket connects on startup
1. Launch the app
2. Observe the network tab in DevTools
3. Look for a WebSocket upgrade request to `ws://localhost:<port>/ws?auth=Session...`
Expected: WS connection opens within 5 seconds. Connection shows `101 Switching Protocols`.

### TC-WS-02: Clipboard new-content event received via WebSocket (cross-device)
1. On Device A: open clipboard history page
2. On Device B: copy any text to the clipboard
3. Observe Device A's clipboard history list
Expected: Device A receives `clipboard.new-content` WebSocket message within 100ms. New entry appears at top of list. No Tauri emit involved.

### TC-WS-03: Pairing verification event received via WebSocket
1. Start the pairing process (Settings > Devices > Add Device)
2. On the peer device, initiate a pairing request to this device
3. Observe the pairing confirmation dialog on this device
Expected: `pairing.verificationRequired` arrives via daemon WebSocket. Pairing verification UI appears. `pairing.complete` arrives after confirmation.

### TC-WS-04: Encryption ready event received via WebSocket
1. Open DevTools console
2. Refresh the app
3. Observe console output related to encryption state
Expected: `encryption.sessionReady` arrives via daemon WebSocket. App proceeds to main dashboard without hanging on "Initializing encryption".

### TC-WS-05: WebSocket auto-reconnects after daemon restart
1. Kill the daemon process (SIGTERM)
2. Wait 3 seconds
3. Restart the daemon
4. Observe the WS connection in DevTools
Expected: Frontend reconnects with exponential backoff, resumes receiving events. UI continues normally. No manual refresh needed.

### TC-WS-06: Multiple concurrent subscriptions work without conflict
1. Open the clipboard history page
2. Simultaneously, trigger a pairing flow (if another device is available)
3. Observe both subscriptions simultaneously
Expected: Both subscriptions fire their respective callbacks correctly. No event goes to wrong handler.

### TC-WS-07: Subscriptions survive page navigation
1. Navigate from clipboard history to settings page
2. Navigate back to clipboard history
3. Copy text on another device
4. Observe clipboard list on return
Expected: Subscription re-established on each mount. New entries appear correctly after returning.

### TC-WS-08: Unsubscribe called on unmount (no memory leaks)
1. Navigate to clipboard history page (hooks subscribe)
2. Navigate away (hooks unmount)
3. Trigger clipboard event on another device
4. Return to clipboard history page
Expected: No duplicate events or handler accumulation. Re-subscribing works correctly.

## Edge Cases

### EC-WS-01: No `daemon://connection-info` event fires (stall detection)
Expected: daemonWs never connects. All WS-dependent features hang silently. Console shows error in daemon-ws-bootstrap.ts.

### EC-WS-02: Daemon WS endpoint does not read `?auth` token
Expected: Connection opens but daemon rejects session. daemonWs enters reconnect loop. Console shows repeated connection attempts.

## Pass Criteria
| Criterion | Evidence |
|---|---|
| WS connects on startup | TC-WS-01: WS upgrade request visible in DevTools network tab |
| Cross-device clipboard events via WS | TC-WS-02: New entry appears within 100ms, no Tauri emit |
| Pairing events via WS | TC-WS-03: `pairing.verificationRequired` triggers UI without Tauri event |
| Encryption ready via WS | TC-WS-04: App proceeds past encryption init via WS event |
| Reconnect resilience | TC-WS-05: App continues after daemon restart without refresh |
| No subscription conflicts | TC-WS-06: Multiple topics fire correct handlers simultaneously |
| No memory leaks | TC-WS-08: Unsubscribed handlers do not fire after unmount |
