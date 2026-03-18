# Phase 43: CLI Daemon Management — Research

**Researched:** 2026-03-19
**Domain:** Process management, daemon lifecycle control, log viewing via CLI
**Confidence:** HIGH

## Summary

Phase 43 adds `daemon start`, `daemon stop`, `daemon restart`, and `daemon logs` subcommands to the existing `uniclipboard-cli` binary. The daemon binary (`uniclipboard-daemon`) already exists with full lifecycle support (Phase 41): it binds a Unix socket, starts workers, handles SIGTERM/Ctrl-C gracefully, and cleans up the socket on exit. The CLI already connects to the daemon via JSON-RPC over Unix socket for the `status` command.

The core challenge is process management: spawning the daemon as a detached background process from the CLI, tracking it via a PID file, and sending signals to stop/restart it. Log viewing requires reading the existing JSON log file that the tracing subsystem already writes. No new daemon-side RPC methods are needed for start/stop — process signals are the standard mechanism. A `shutdown` RPC method is useful as a graceful alternative to SIGTERM.

**Primary recommendation:** Use `std::process::Command` with `setsid` (via `pre_exec` on Unix) for daemon spawning, a simple PID file alongside the socket file for process tracking, SIGTERM for stop, and sequential stop+start for restart. Add a `shutdown` RPC command as a graceful stop path. For `logs`, tail the existing JSON log file with optional `--follow` mode.

## Standard Stack

### Core

| Library               | Version       | Purpose                                   | Why Standard                                        |
| --------------------- | ------------- | ----------------------------------------- | --------------------------------------------------- |
| std::process::Command | stdlib        | Spawn daemon process                      | No external dep needed; `pre_exec` hook for setsid  |
| std::fs               | stdlib        | PID file read/write/remove                | Simple file operations, no crate needed             |
| tokio::signal         | 1.x (bundled) | Signal handling in daemon                 | Already used in daemon's `wait_for_shutdown_signal` |
| nix                   | 0.31.2        | `setsid()`, `kill()` for Unix process ops | Well-maintained, Rust-idiomatic POSIX API           |

### Supporting

| Library | Version | Purpose                        | When to Use                                    |
| ------- | ------- | ------------------------------ | ---------------------------------------------- |
| sysinfo | 0.38.4  | Process existence check by PID | Only if `kill(pid, 0)` via nix is insufficient |

### Alternatives Considered

| Instead of                  | Could Use                  | Tradeoff                                                                                                                                |
| --------------------------- | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| std::process::Command + nix | `fork` crate 0.7.0         | fork does double-fork daemonization but adds dep; Command+setsid is simpler for our case since daemon already handles its own lifecycle |
| nix::sys::signal::kill      | `libc::kill` directly      | nix provides safer Rust API; libc is lower-level                                                                                        |
| PID file                    | Socket-only liveness check | PID file enables `stop` without RPC; socket check alone can't send SIGTERM to a specific PID                                            |
| `daemonize` crate           | —                          | RUSTSEC-2025-0069: unmaintained since June 2023, do NOT use                                                                             |

**Installation:**

```bash
# In uc-cli/Cargo.toml (nix is only needed in uc-cli, not uc-daemon)
# nix with "signal" and "process" features
cd src-tauri && cargo add nix --features signal,process -p uc-cli
```

**Version verification:** nix 0.31.2 confirmed via `cargo search nix` on 2026-03-19.

## Architecture Patterns

### Recommended Project Structure

```
src-tauri/crates/
├── uc-cli/src/
│   ├── commands/
│   │   ├── mod.rs              # Add daemon module
│   │   ├── daemon/
│   │   │   ├── mod.rs          # DaemonCmd enum (Start, Stop, Restart, Logs)
│   │   │   ├── start.rs        # Spawn daemon, write PID file
│   │   │   ├── stop.rs         # Read PID, send SIGTERM, wait, cleanup
│   │   │   ├── restart.rs      # stop() then start()
│   │   │   └── logs.rs         # Tail log file
│   │   ├── status.rs           # (existing)
│   │   ├── devices.rs          # (existing)
│   │   └── space_status.rs     # (existing)
│   ├── pid.rs                  # PID file read/write/check helpers
│   └── main.rs                 # Add Daemon subcommand group
├── uc-daemon/src/
│   ├── rpc/handler.rs          # Add "shutdown" RPC method
│   └── (rest unchanged)
```

### Pattern 1: PID File Management

**What:** Write daemon PID to a file next to the socket; read it for stop/restart.
**When to use:** Every start/stop/restart operation.
**Example:**

```rust
// PID file lives alongside socket: e.g., /tmp/uniclipboard-daemon.pid
// Uses same resolve_daemon_socket_path() base directory

use std::path::PathBuf;
use uc_daemon::socket::resolve_daemon_socket_path;

pub fn resolve_daemon_pid_path() -> PathBuf {
    let socket_path = resolve_daemon_socket_path();
    socket_path.with_extension("pid")
    // e.g., /tmp/uniclipboard-daemon.pid
}

pub fn write_pid_file(pid: u32) -> std::io::Result<()> {
    std::fs::write(resolve_daemon_pid_path(), pid.to_string())
}

pub fn read_pid_file() -> Option<u32> {
    std::fs::read_to_string(resolve_daemon_pid_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

pub fn remove_pid_file() {
    let _ = std::fs::remove_file(resolve_daemon_pid_path());
}
```
