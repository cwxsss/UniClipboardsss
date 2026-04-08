---
estimated_steps: 8
estimated_files: 1
skills_used: []
---

# T02: WebSocket event delivery and reconnect tests

Test DaemonWsClient event delivery:

- Subscribe to 'clipboard' topic, copy on device B → event received within 100ms
- Subscribe to 'encryption' topic, lock/unlock → events received
- Kill daemon process → daemonWs reconnects with exponential backoff
- Restart daemon → frontend auto-resubscribes, data refreshes
- Multiple rapid events → all delivered in order
- Unsubscribe → no further events received

Use a test daemon instance or mock WebSocket server.

## Inputs

- `src/lib/daemon-ws.ts`

## Expected Output

- `src/__tests__/lib/daemon-ws.test.ts`

## Verification

All WS tests pass. Event latency measured and within 100ms threshold.
