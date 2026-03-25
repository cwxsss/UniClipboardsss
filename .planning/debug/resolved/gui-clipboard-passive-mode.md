---
status: resolved
trigger: 'After Phase 57 changes, copying content no longer triggers automatic frontend updates'
created: 2026-03-25T00:00:00Z
updated: 2026-03-25T06:30:00Z
resolved: 2026-03-25T06:30:00Z
fix_commit: 91e2e49b
---

## Current Focus

hypothesis: GUI hardcoded to Passive mode unconditionally (line 195 runtime.rs), disabling clipboard watcher even when no daemon is running
test: Read code to confirm hardcoded Passive and trace watcher startup
expecting: Passive mode causes StartClipboardWatcher to no-op
next_action: Apply fix - use UC_CLIPBOARD_MODE env var (like non_gui_runtime) with Full as default

## Symptoms

expected: Clipboard list in GUI refreshes when new content is copied
actual: No frontend updates on clipboard copy
errors: None visible (silent no-op)
reproduction: Run `bun tauri dev`, copy any content, observe clipboard list does not update
started: After Phase 57 changes (57-02 hardcoded GUI to Passive mode)

## Eliminated

(none)

## Evidence

- timestamp: 2026-03-25T00:00:00Z
  checked: runtime.rs line 193-195
  found: `let clipboard_integration_mode = uc_core::clipboard::ClipboardIntegrationMode::Passive;` hardcoded unconditionally
  implication: GUI always runs in Passive mode regardless of daemon presence

- timestamp: 2026-03-25T00:00:00Z
  checked: start_clipboard_watcher.rs line 68-71
  found: `if !self.mode.observe_os_clipboard() { return Ok(()); }` - Passive mode returns early
  implication: Clipboard watcher never starts in Passive mode

- timestamp: 2026-03-25T00:00:00Z
  checked: integration_mode.rs line 11-13
  found: `observe_os_clipboard()` returns false for Passive
  implication: Confirms the chain: Passive -> no watcher -> no clipboard capture

- timestamp: 2026-03-25T00:00:00Z
  checked: non_gui_runtime.rs line 129, 199-202
  found: Non-GUI runtime uses `resolve_clipboard_integration_mode()` which reads UC_CLIPBOARD_MODE env var, defaults to Full
  implication: The env-var-based approach already exists and works correctly

## Resolution

root_cause: Phase 57-02 hardcoded GUI ClipboardIntegrationMode to Passive unconditionally (runtime.rs line 195). In standalone dev mode (no daemon), this disables the clipboard watcher entirely since StartClipboardWatcher is a no-op in Passive mode.
fix: Replace hardcoded Passive with the same env-var-based resolution used by non-GUI runtime (resolve_clipboard_integration_mode from uc-bootstrap). Default is Full, allowing standalone GUI to work. Set UC_CLIPBOARD_MODE=passive when running with daemon.
verification: cargo check passes, all relevant tests pass (uc-bootstrap clipboard mode tests, uc-platform watcher tests, 78/79 uc-tauri tests - 1 pre-existing failure unrelated)
files_changed: [src-tauri/crates/uc-tauri/src/bootstrap/runtime.rs, src-tauri/crates/uc-bootstrap/src/non_gui_runtime.rs, src-tauri/crates/uc-bootstrap/src/lib.rs]
