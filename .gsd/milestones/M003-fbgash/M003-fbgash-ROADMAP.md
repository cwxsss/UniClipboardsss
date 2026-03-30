# M003-fbgash: 

## Vision
Replace Tauri invoke() as the primary transport between frontend and backend with direct daemon HTTP client + WebSocket connection. Frontend communicates directly with daemon HTTP API for all business operations and subscribes to daemon WebSocket for real-time events. The uc-tauri layer becomes a thin shell handling only Tauri-native features (tray, updater, autostart, protocol handler).

## Slice Overview
| ID | Slice | Risk | Depends | Done | After this |
|----|-------|------|---------|------|------------|
| S01 | Frontend Daemon HTTP Client & Auth Module | medium | — | ✅ | DaemonClient singleton in src/api/daemon/client.ts; loadDaemonAuth() and verifyAuthState() in src/lib/daemon-auth.ts; session refresh every 4min |
| S02 | Frontend Clipboard API Migration | medium | S01 | ✅ | Clipboard list page loads entries via GET /clipboard/entries; restore sends POST; entries update in real-time via WS events |
| S03 | Frontend WebSocket Direct Connection & Event Migration | high | S01 | ✅ | Frontend connects to daemon WS directly; clipboard new-content events trigger UI update without Tauri emit |
| S04 | uc-tauri Command Cleanup | high | S02, S03 | ✅ | uc-tauri/src/commands/clipboard.rs, encryption.rs, settings.rs, storage.rs deleted; invoke_handler![] cleaned up |
| S05 | Frontend-Daemon Integration Testing & Security Audit | medium | S04 | ⬜ | Test suite runs: HTTP API correctness, WS event delivery, session token lifecycle, reconnection recovery, security properties |
