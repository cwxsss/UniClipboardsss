---
estimated_steps: 5
estimated_files: 1
skills_used: []
---

# T04: Auth module bridging Tauri bootstrap and daemon HTTP

Create `src/lib/daemon-auth.ts`:

`loadDaemonAuth()`: call Tauri invoke for connection config → call DaemonClient.refreshSession() → return session token. Also extract wsUrl for later WS connection.

`verifyAuthState()`: check daemon is reachable (GET /lifecycle/ready or similar health check) and encryption status.

`waitForEncryptionReady(timeout)`: poll GET /encryption/state every 500ms until encryptionReady===true or timeout. Return boolean.

This module bridges Tauri IPC (for bootstrap) and daemon HTTP (for all subsequent calls).

## Inputs

- `src/api/daemon/client.ts`
- `src-tauri/crates/uc-daemon/src/api/encryption.rs`

## Expected Output

- `src/lib/daemon-auth.ts`

## Verification

Unit tests: loadDaemonAuth calls both Tauri and daemon HTTP. verifyAuthState returns correct state. waitForEncryptionReady resolves on ready, rejects on timeout.
