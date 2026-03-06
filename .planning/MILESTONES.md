# Milestones

## v0.1.0 — Daily Driver

**Shipped:** 2026-03-06
**Phases:** 9 | **Plans:** 17 | **Tasks:** ~30
**Timeline:** 5 days (2026-03-02 → 2026-03-06)
**Files changed:** 204 (+23,577 / -1,703)

### Delivered

Evolved UniClipboard from baseline LAN clipboard sync into a production-ready daily driver with unified encrypted transfer, true streaming inbound decode, optimized at-rest blob format, reliable Windows image capture, cross-platform dashboard image rendering, setup-flow UX redesign, V3 binary sync protocol with zero-copy fanout, and large-image clipboard read/memory optimizations.

### Accomplishments

1. Unified transfer protocol (UTL-01..07) with per-chunk XChaCha20-Poly1305 encryption and V1 compatibility.
2. Two-segment wire framing + transport-level streaming decode to remove `read_to_end` bottleneck.
3. Binary V2 blob-at-rest format + zstd compression + migration and spool cleanup flow.
4. Windows image capture stabilized via clipboard-rs upgrade and native CF_DIB fallback.
5. V3 binary sync pipeline delivered (compression + Arc zero-copy fanout + V3-only inbound rewrite).
6. macOS large-image read pipeline optimized (TIFF alias dedup + deferred conversion), reducing peak memory.

### Known Gaps

- Milestone audit status for original v0.1 scope was `tech_debt` (not strict `passed`).
- Phase 01: Missing formal VERIFICATION.md (informal verification recorded in SUMMARY).
- Pre-existing test failure: `business_command_timeouts_cover_stream_operation_budgets`.
- Human verification pending: multi-device memory profile and E2E inbound with active encryption session.
- `broadcast_clipboard` port remains unimplemented (pre-existing).
- `.planning/REQUIREMENTS.md` was not maintained as active file during phases 4-9; requirements were reconstructed from roadmap/summaries.

### Git

- Range: `8304405..bab0ae7`
- Tag: `v0.1.0`
