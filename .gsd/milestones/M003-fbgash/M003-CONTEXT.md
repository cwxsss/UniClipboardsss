# M003: Frontend Direct Daemon Connection — Context

**Gathered:** 2026-03-30
**Status:** Ready for planning
**Source:** PRD Express Path (docs/plans/frontend-direct-daemon-connection.md)
**Migrated from:** .planning/phases/77–81

## Phase Boundary

Replace Tauri invoke() as the primary transport between frontend and backend with direct daemon HTTP client + WebSocket. Frontend communicates directly with daemon HTTP API for all business operations (clipboard, settings, encryption, storage) and subscribes to daemon WebSocket for real-time events. The uc-tauri layer becomes a thin shell handling only Tauri-native features (tray, updater, autostart, protocol handler, daemon lifecycle).

## Motivation

The current architecture routes all frontend-to-backend communication through Tauri IPC (invoke() / listen()). The daemon owns the actual business logic and storage. The Tauri IPC layer is largely a pass-through. Direct daemon connection:

1. Eliminates double-hop: frontend → Tauri IPC → daemon → Tauri IPC → frontend
2. Enables daemon to run as a true standalone service
3. Reduces uc-tauri code by 60%+
4. Simplifies the architecture to a clean client-server model

## Architecture

```
Before (Tauri IPC hop):
  Frontend → invoke() → uc-tauri commands → uc-daemon-client → daemon → response
  Daemon → WS event → uc-daemon-client → uc-tauri emit() → listen() → Frontend

After (direct daemon connection):
  Frontend → Tauri daemon_connect_info (bootstrap only) → DaemonClient
  DaemonClient → HTTP → daemon → response
  daemon → WS event → DaemonWsClient → Frontend hooks
```

## Deferred Phases

- Offline/disconnected state handling (later)
- Settings/storage WS event subscriptions (later)
- Windows/Linux-specific edge cases (later)
- Automated security scanning tools integration (later)

---

_Original phases: 77, 78, 79, 80, 81_
