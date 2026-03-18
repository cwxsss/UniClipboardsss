# Phase 44: CLI Settings Commands - Research

**Researched:** 2026-03-19
**Domain:** CLI settings get/set/list via existing uc-core Settings model + uc-bootstrap
**Confidence:** HIGH

## Summary

Phase 44 adds three CLI subcommands -- `settings list`, `settings get <key>`, and `settings set <key> <value>` -- to the existing `uc-cli` crate. The entire infrastructure is already in place: `SettingsPort` trait in uc-core (load/save), `FileSettingsRepository` in uc-infra (atomic JSON file persistence), `GetSettings` and `UpdateSettings` use cases in uc-app, and the `build_cli_context_with_profile` + `build_non_gui_runtime` bootstrap path in uc-bootstrap. The CLI already has three working direct-mode commands (`devices`, `space-status`, `status`) that demonstrate the exact pattern to follow.

The Settings struct serializes to a nested JSON structure with 7 top-level sections (`general`, `sync`, `retention_policy`, `security`, `pairing`, `keyboard_shortcuts`, `file_sync`) plus `schema_version`. Dot-notation key paths (e.g., `general.auto_start`, `sync.max_file_size_mb`) map naturally to `serde_json::Value::pointer()` access by converting dots to slashes. No new crate dependencies are needed.

The `set` command requires a read-modify-write cycle: load full Settings via `GetSettings`, convert to `serde_json::Value`, mutate the target field, deserialize back to `Settings`, then persist via `UpdateSettings`. This ensures schema validation and the diff-logging in `UpdateSettings` remain active. The Tauri `update_settings` command does additional side-effects (autostart, device name announce, shortcut re-registration, event broadcast) that are GUI-specific and should NOT be replicated in CLI -- the CLI just persists the value.

**Primary recommendation:** Add a `Settings` clap subcommand with `list`, `get`, and `set` sub-subcommands. Use serde_json Value-level access for dot-notation keys. Reuse existing `GetSettings`/`UpdateSettings` use cases. Follow the established `devices.rs` command pattern exactly.

## Standard Stack

### Core (already in workspace)

| Library      | Version   | Purpose                                   | Why Standard           |
| ------------ | --------- | ----------------------------------------- | ---------------------- |
| clap         | 4.5       | CLI argument parsing with derive          | Already used in uc-cli |
| serde_json   | 1.0       | JSON serialization + Value pointer access | Already a dependency   |
| serde        | 1.0       | Serialize/Deserialize traits              | Already a dependency   |
| uc-bootstrap | workspace | CLI context + non-GUI runtime builder     | Established pattern    |
| uc-app       | workspace | GetSettings + UpdateSettings use cases    | Established pattern    |
| uc-core      | workspace | Settings model + SettingsPort trait       | Domain layer           |

### No New Dependencies Needed

All required functionality exists in the current dependency set. `serde_json::Value::pointer()` provides JSON Pointer (RFC 6901) access for dot-notation key paths after converting dots to slashes.

## Architecture Patterns

### Recommended Project Structure
