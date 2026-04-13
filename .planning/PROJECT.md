# UniClipboard Desktop

## What This Is

A cross-platform clipboard synchronization app built with Tauri 2, React, and Rust. It provides encrypted LAN clipboard sync for text, images, links, and files, local encrypted clipboard search, CLI and daemon runtime modes, and observability tooling for debugging and support.

## Core Value

Seamless clipboard synchronization across devices — users can copy on one device and paste on another without interrupting their workflow.

## Current Milestone

No active milestone is currently defined.

## Current State

- **Latest shipped milestone:** v0.5.0 Local Encrypted Search (archived 2026-04-13)
- **Current capability level:** Full-featured clipboard sync with text, image, link, and file support; structured observability; per-device sync control; CLI clipboard history and search commands; daemon auto-recovers encryption session on startup; local encrypted search is available through daemon routes, CLI commands, QuickPanel keyword search, and Dashboard search with content-type and time-range filters
- **Architecture status:** Hexagonal architecture with compiler-enforced boundaries, typed command surfaces, lifecycle governance, daemon-first runtime ownership, and consolidated sync planning
- **LOC:** ~324K Rust + ~39K TypeScript (estimated)
- **Supported content types:** Text, Image, Link, File
- **Archive note:** v0.5.0 was archived after Phase 93 was completed manually and planning records were backfilled during archive; no separate milestone audit file exists for this milestone

## Next Milestone Goals

- Define the next milestone before new phase execution starts
- Carry forward any post-release cleanup that does not belong in the shipped v0.5.0 archive
- Reintroduce a fresh `.planning/REQUIREMENTS.md` as part of next-milestone setup

## Requirements

### Validated

- ✓ Clipboard text capture and history — existing
- ✓ Device pairing and LAN sync baseline — existing
- ✓ V2 unified transfer and streaming decode foundation — v0.1.0
- ✓ At-rest blob format optimization and migration — v0.1.0
- ✓ Windows image clipboard capture reliability — v0.1.0
- ✓ Dashboard image display compatibility across platforms — v0.1.0
- ✓ Setup flow UX consistency improvements — v0.1.0
- ✓ V3 binary sync protocol, compression, and zero-copy fanout — v0.1.0
- ✓ Large-image clipboard read pipeline memory/latency improvements — v0.1.0
- ✓ Cross-layer boundary violation removal and command-layer penetration closure — v0.2.0
- ✓ Typed command DTO/error contracts and traceable API surfaces — v0.2.0
- ✓ Lifecycle governance (task cancellation, graceful shutdown, runtime cleanup) — v0.2.0
- ✓ God-object decomposition (AppDeps/SetupOrchestrator/PairingOrchestrator) — v0.2.0
- ✓ Test infrastructure consolidation (shared noop ports) — v0.2.0
- ✓ Dashboard incremental update with origin-based event routing — v0.2.0
- ✓ Runtime theme preset engine with multi-dot Appearance swatches — v0.2.0
- ✓ Dual-output structured logging with configurable profiles (dev/prod/debug_clipboard) — v0.3.0
- ✓ Flow correlation with flow_id/stage spans across clipboard capture and sync pipelines — v0.3.0
- ✓ Seq local integration with CLEF format, async batching, and cross-device tracing — v0.3.0
- ✓ Per-device sync settings with content type toggles and global master toggle — v0.3.0
- ✓ File sync via libp2p with chunked transfer, Blake3 verification, and retry logic — v0.3.0
- ✓ File clipboard integration with auto-write, stale detection, and delete cascade — v0.3.0
- ✓ File sync UI (Dashboard entries, context menu, progress, notifications) — v0.3.0
- ✓ File sync settings with quota enforcement and auto-cleanup — v0.3.0
- ✓ File sync eventual consistency with durable transfer lifecycle tracking — v0.3.0
- ✓ Link content type detection and display with per-device sync toggle — v0.3.0
- ✓ Keyboard shortcuts settings UI with click-to-record and conflict detection — v0.3.0
- ✓ macOS keychain auto-unlock confirmation modal — v0.3.0
- ✓ Event-driven device discovery replacing polling — v0.3.0
- ✓ Unified CLI/GUI/Daemon auth architecture (session exchange via /auth/connect, bare bearer rejection on L2+ routes, independent token scopes) — v0.4.0
- ✓ OpenTelemetry OTLP/HTTP-protobuf pipeline replaces legacy Seq/CLEF layer and propagates trace context across device sync boundaries — v0.4.0
- ✓ Local encrypted clipboard search backend with HKDF-derived keying, HMAC-tagged index terms, exact keyword search, time-range filtering, file-type filtering, rebuild flow, and locked-session enforcement — v0.5.0
- ✓ Local encrypted clipboard search UI across QuickPanel and Dashboard, including result counts and time-range controls — v0.5.0

### Active

No active milestone is currently defined.

### Deferred

- [ ] Complete chunked transfer resume protocol (CT-02, CT-04 — backend only, frontend deferred)
- [ ] Wire transfer progress events to frontend UI (CT-05)
- [ ] Add favorites persistence (domain model column needed)
- [ ] Wire lifecycle events to frontend (currently polling, not event-driven)
- [ ] Expand typed error migration to port surfaces (ARCHNEXT-01)
- [ ] Domain model refinement for anemic models (ARCHNEXT-02)
- [ ] Collector & multi-backend support for observability
- [ ] WebDAV cross-internet sync
- [ ] Runtime log profile switching (OBS-01)

### Out of Scope

- Mobile app — desktop-first
- OAuth/third-party login — not required for current product model
- Remote/cloud log shipping — clipboard logs may contain sensitive content
- In-app log viewer UI — Seq provides dedicated log UI

## Context

Archived v0.5.0 on 2026-04-13 after local encrypted search shipped across backend, CLI, QuickPanel, and Dashboard.
Tech stack: Tauri 2 + React 18 + Rust + libp2p + XChaCha20-Poly1305.
Hexagonal boundaries are compiler-enforced. Daemon is the main runtime authority for sync, search, and space state.
Clipboard search now supports exact keyword search, time-range presets, content-type filters, file-extension filters, rebuild operations, and locked-session handling.

## Key Decisions

| Decision | Rationale | Outcome |
| --- | --- | --- |
| Two-segment framing for clipboard wire format | Reduce overhead and enable stream decode | ✓ Good |
| V3 binary protocol with Arc fanout | Improve large payload performance and memory behavior | ✓ Good |
| Manual uc:// URL resolution strategy | Ensure Windows/WebView compatibility | ✓ Good |
| Background TIFF conversion | Keep clipboard capture path responsive | ✓ Good |
| Private deps + facade accessors on AppRuntime | Compiler-enforced boundary: commands cannot access internals | ✓ Good |
| CommandError serde tag=code content=message | Frontend discriminated union handling | ✓ Good |
| TaskRegistry with CancellationToken cascade | Deterministic shutdown without orphaned tasks | ✓ Good |
| OutboundSyncPlanner consolidation | Single policy decision point, runtime as thin dispatcher | ✓ Good |
| HKDF-derived search key + HMAC-tagged exact-token index | Preserve local search without storing plaintext terms | ✓ Good |
| Search rebuild via version-flag atomic swap | Avoid SQLite table rename lock contention | ✓ Good |

## Constraints

- **Tech stack:** Tauri 2 + React + Rust
- **Sync domain:** LAN-first with libp2p
- **Security:** XChaCha20-Poly1305 remains mandatory
- **Platform support:** macOS primary; Windows/Linux supported

---

_Last updated: 2026-04-13 after v0.5.0 milestone archive_
