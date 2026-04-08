---
estimated_steps: 1
estimated_files: 6
skills_used: []
---

# T01: Add daemon clipboard contract coverage for clear-history, detail/resource, and favorite semantics

Close the backend/client contract gaps that still force clipboard business flows through Tauri. Add a confirmed daemon clear-history route, expose typed daemon wrappers for clear/detail/resource, and align the favorite toggle method with the real daemon route so frontend code can migrate without transport fallbacks.

## Inputs

- ``src-tauri/crates/uc-daemon/src/api/clipboard.rs``
- ``src-tauri/crates/uc-app/src/usecases/clipboard/clear_history.rs``
- ``src/api/daemon/clipboard.ts``
- ``src/api/daemon/__tests__/clipboard.test.ts``

## Expected Output

- ``src-tauri/crates/uc-daemon/src/api/clipboard.rs``
- ``src-tauri/crates/uc-daemon/tests/clipboard_api.rs``
- ``src/api/daemon/clipboard.ts``
- ``src/api/daemon/__tests__/clipboard.test.ts``

## Verification

cd src-tauri && cargo test -p uc-daemon clipboard_api -- --nocapture && npx vitest run src/api/daemon/__tests__/clipboard.test.ts

## Observability Impact

Route tests make destructive clear rejection codes and detail/resource error contracts explicit, so later failures localize to daemon routing vs frontend call sites.
