---
status: resolved
trigger: "Multiple 'database is locked' errors occurring when concurrent operations compete for SQLite access"
created: 2026-03-08T00:00:00Z
updated: 2026-03-08T00:00:00Z
---

## Current Focus

hypothesis: SQLite connections lack WAL mode and busy_timeout pragmas, causing "database is locked" under concurrent read/write
test: Add connection_customizer to set WAL mode + busy_timeout on each new connection
expecting: Concurrent reads (list_entry_projections) and writes (background_blob_worker) no longer conflict
next_action: Implement CustomizeConnection for r2d2 pool in pool.rs

## Symptoms

expected: All clipboard entries should load successfully in the dashboard, and background blob worker should update representation states without errors.
actual: Multiple entries are skipped due to "database is locked" errors. The background_blob_worker also fails to update representation state and retries.
errors:

- "Skipping entry due to preview representation lookup failure... error=Database error: database is locked"
- "Failed to query clipboard_selection for entry_id: database is locked"
- "Failed to update representation state after blob write... error=Database error: database is locked"
- "Processing failed; retrying attempt=1 max_attempts=5 error=Database error: database is locked"
  reproduction: Occurs during normal clipboard dashboard usage when background blob processing happens simultaneously with entry listing.
  started: Recent, likely introduced with clipboard management feature additions (phase-15).

## Eliminated

(none yet)

## Evidence

- timestamp: 2026-03-08T00:01:00Z
  checked: src-tauri/crates/uc-infra/src/db/pool.rs - init_db_pool function
  found: Pool::builder().build(manager) with NO connection_customizer, NO pragmas set. Default r2d2 pool (max_size=10 connections). No WAL mode, no busy_timeout.
  implication: SQLite default journal mode is DELETE (not WAL). In DELETE mode, any writer blocks ALL readers. With 10 pool connections and concurrent async tasks, "database is locked" is expected.

- timestamp: 2026-03-08T00:02:00Z
  checked: Entire codebase for pragma/wal/busy_timeout/connection_customizer
  found: Zero hits. No SQLite pragmas are set anywhere except foreign_keys in migrations.
  implication: Confirms root cause - WAL mode and busy_timeout are completely missing.

- timestamp: 2026-03-08T00:03:00Z
  checked: src-tauri/crates/uc-infra/src/db/executor.rs - DieselSqliteExecutor
  found: Simple pool.get() + closure execution. No per-connection setup.
  implication: Each connection from the pool has default SQLite settings.

## Resolution

root_cause: SQLite connections are created with default journal_mode (DELETE) and no busy_timeout. In DELETE mode, a single writer blocks all readers. The background_blob_worker writes concurrently with get_clipboard_entries reads, causing immediate "database is locked" errors. WAL (Write-Ahead Logging) mode allows concurrent readers + one writer, and busy_timeout tells SQLite to retry internally instead of immediately returning SQLITE_BUSY.

fix: Add r2d2::CustomizeConnection implementation that sets PRAGMA journal_mode=WAL and PRAGMA busy_timeout=5000 on each new connection from the pool.
verification: User confirmed fix working in real environment - no more "database is locked" errors during concurrent dashboard + blob worker usage.
files_changed: [src-tauri/crates/uc-infra/src/db/pool.rs]
