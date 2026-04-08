---
estimated_steps: 5
estimated_files: 1
skills_used: []
---

# T06: Encryption API module

Create `src/api/daemon/encryption.ts`:

`getEncryptionState()` → GET /encryption/state
`unlockEncryption(passphrase)` → POST /encryption/unlock with { passphrase }
`lockEncryption()` → POST /encryption/lock

All via DaemonClient.request(). Return typed EncryptionStateResponse.

## Inputs

- `src-tauri/crates/uc-app/src/dtos/encryption.rs (response shapes)`

## Expected Output

- `src/api/daemon/encryption.ts`

## Verification

TypeScript compiles. Integration test against running daemon.
