# Phase 45: CLI Sync Commands - Research

**Researched:** 2026-03-19
**Domain:** CLI sync management commands (force, pause, resume, status) via daemon RPC
**Confidence:** HIGH

## Summary

Phase 45 adds four CLI subcommands — `sync force`, `sync pause`, `sync resume`, and `sync status` — to the existing `uc-cli` crate. Unlike phases 42-44 which use direct-mode bootstrap (no daemon required), sync commands are fundamentally daemon-dependent: sync operations require a running daemon with active network connections, clipboard watcher, and peer discovery workers. Therefore, all four commands use the daemon RPC pattern established by `status.rs` in Phase 41.

The current daemon infrastructure has a JSON-RPC server over Unix domain sockets with method dispatch in `rpc/handler.rs`. The `RpcRequest` type currently has `jsonrpc`, `method`, and `id` fields but no `params` field — this needs to be added to support parameterized commands. The daemon's `RuntimeState` is a snapshot-only struct tracking uptime and worker health; it needs to be extended with sync-specific state (paused flag, last sync timestamp, sync statistics).

The existing sync infrastructure in uc-app includes `SyncOutboundClipboardUseCase`, `SyncInboundClipboardUseCase`, `OutboundSyncPlanner`, and file sync use cases. However, the daemon workers are currently placeholders (Phase 46 wires real clipboard capture). This means Phase 45 should focus on the RPC protocol, CLI command structure, and daemon-side state management, with the actual sync triggering wired when workers become real in Phase 46.

**Primary recommendation:** Add `sync_force`, `sync_pause`, `sync_resume`, and `sync_status` RPC methods to the daemon handler. Extend `RpcRequest` with an optional `params` field. Add sync state tracking to `RuntimeState`. Create a `sync.rs` CLI command module following the `status.rs` RPC pattern.

## Standard Stack

### Core (already in workspace)

| Library            | Version | Purpose                                      | Why Standard           |
| ------------------ | ------- | -------------------------------------------- | ---------------------- |
| clap               | 4.5     | CLI argument parsing with derive             | Already used in uc-cli |
| serde + serde_json | 1.x     | JSON serialization for RPC and --json output | Already used           |
| tokio              | 1.x     | Async runtime + Unix socket I/O              | Already used           |
| anyhow             | 1.0     | Error handling                               | Already used           |
| uc-daemon          | 0.1.0   | RPC types, socket path, RuntimeState         | Daemon library         |
| uc-cli             | 0.1.0   | CLI binary with existing command patterns    | CLI crate              |
| chrono             | 0.4     | Timestamp formatting for last_sync_at        | Already in workspace   |

### No New Dependencies Needed

All required functionality exists in the current dependency set. `chrono` is already available in the workspace for timestamp handling. The `RpcRequest`/`RpcResponse` types in uc-daemon handle all RPC communication.

## Architecture Patterns

### Recommended Project Structure

```
src-tauri/crates/uc-cli/src/
├── commands/
│   ├── mod.rs              # Add: pub mod sync;
│   ├── sync.rs             # NEW: force, pause, resume, status handlers
│   ├── status.rs           # Existing (RPC pattern reference)
│   ├── devices.rs          # Existing (direct pattern)
│   ├── space_status.rs     # Existing (direct pattern)
│   └── settings.rs         # Added in Phase 44
├── main.rs                 # Add Sync subcommand group
├── exit_codes.rs           # Existing (reuse)
└── output.rs               # Existing (reuse)

src-tauri/crates/uc-daemon/src/
├── rpc/
│   ├── types.rs            # MODIFY: add params to RpcRequest, add SyncStatusResponse
│   ├── handler.rs          # MODIFY: add sync_force, sync_pause, sync_resume, sync_status handlers
│   ├── server.rs           # Unchanged
│   └── mod.rs              # Unchanged
├── state.rs                # MODIFY: add SyncState to RuntimeState
├── worker.rs               # Unchanged
├── workers/
│   ├── clipboard_watcher.rs # Unchanged (placeholder until Phase 46)
│   └── peer_discovery.rs    # Unchanged (placeholder until Phase 46)
├── app.rs                  # Unchanged
├── socket.rs               # Unchanged
└── lib.rs                  # Unchanged
```
