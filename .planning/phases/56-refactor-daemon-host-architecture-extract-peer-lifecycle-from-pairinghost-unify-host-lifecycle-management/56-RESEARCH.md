# Phase 56: Refactor daemon host architecture - Research

**Researched:** 2026-03-23
**Domain:** Rust daemon lifecycle orchestration, pairing host decomposition, peer event projection
**Confidence:** MEDIUM

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

### Responsibility split strategy

- **D-01:** Extract peer lifecycle event handling into a new `PeerMonitor` struct, separate from `PairingHost`. `PeerMonitor` handles: `PeerDiscovered`, `PeerLost`, `PeerConnected`, `PeerDisconnected`, `PeerNameUpdated` → emits WebSocket events. `PairingHost` keeps only: `PairingMessageReceived`, `PairingFailed` → delegates to orchestrator/transport.
- **D-02:** `PeerMonitor` and `PairingHost` each independently call `network_events.subscribe_events()` and filter for their own event types. No central router or dispatcher needed — the existing broadcast channel supports multiple subscribers.
- **D-03:** `PeerMonitor` lives as a peer-level struct alongside `PairingHost` (e.g., new module `peers/monitor.rs` or `peers/peer_monitor.rs`), not nested inside PairingHost.

### Unified service lifecycle

- **D-04:** Merge `DaemonWorker` trait into a new `DaemonService` trait. The interface stays the same: `name()`, `start(CancellationToken)`, `stop()`, `health_check()`. All daemon components — `PairingHost`, `PeerMonitor`, `ClipboardWatcherWorker`, `PeerDiscoveryWorker` — implement `DaemonService`.
- **D-05:** `DaemonApp` holds a single `services: Vec<Arc<dyn DaemonService>>` instead of separate `workers` vec + hardcoded `PairingHost`. The `run()` method spawns all services uniformly and handles shutdown uniformly — no more per-component `completed_xxx_handle` boilerplate.
- **D-06:** Remove `DaemonWorker` trait entirely after migration. Rename the file from `worker.rs` to `service.rs`.
- **D-07:** `DaemonApp` struct fields for pairing-specific state (`pairing_orchestrator`, `pairing_action_rx`, `space_access_orchestrator`, `key_slot_store`) move into `PairingHost` construction before `DaemonApp::new()`. `DaemonApp` only receives `Vec<Arc<dyn DaemonService>>` + `CoreRuntime` + socket path.

### Naming conventions

- **D-08:** Event loop functions are named by responsibility, prefixed by their owning struct's domain:
  - PeerMonitor: `run_peer_event_loop` (or similar, matching peer lifecycle)
  - PairingHost: `run_pairing_protocol_loop` (was `run_pairing_network_event_loop`), `run_pairing_action_loop` (unchanged), `run_pairing_domain_event_loop` (unchanged), `run_pairing_session_sweep_loop` (unchanged)
- **D-09:** The current `PeerDiscoveryWorker` (in `workers/peer_discovery.rs`) may overlap with `PeerMonitor` since both consume NetworkEvent for peer-related concerns. Evaluate whether to merge `PeerDiscoveryWorker`'s announce logic into `PeerMonitor` or keep them separate.

### Claude's Discretion

- Exact module/file organization for `PeerMonitor` (e.g., `peers/` module vs. flat file)
- Whether `PeerDiscoveryWorker`'s device name announcement logic merges into `PeerMonitor` or stays separate
- Internal implementation details of the `DaemonService` trait (e.g., whether `run()` is `self: Arc<Self>` or `&self`)
- Order of service startup/shutdown in `DaemonApp`

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope
</user_constraints>

## Summary

This phase is a structural refactor inside `uc-daemon`, not a behavior rewrite. The current hotspot is clear in code: `DaemonPairingHost` owns pairing lifecycle, peer websocket projection, and its own service-style supervision, while `DaemonApp` still supervises placeholder workers separately and hardcodes pairing-host startup/shutdown in parallel. That leaves the daemon with two lifecycle systems and one misleading event loop that mixes unrelated concerns.

The safest plan is to keep the existing runtime primitives and split only by responsibility boundaries that already exist in `NetworkEvent`. Official Tokio docs confirm `broadcast` is appropriate here: multiple receivers created from the same sender each see values sent after subscription, with lag exposed explicitly rather than hidden. Official `CancellationToken` docs also confirm the exact shutdown model this phase needs: parent cancellation propagates to child tokens, while child cancellation does not flow back upward.

The main planning risk is typed access. `DaemonApp` can supervise `Vec<Arc<dyn DaemonService>>`, but API routes and setup-state queries still need a typed pairing-host handle. That means the composition root must build typed services first, clone the typed `Arc<DaemonPairingHost>` into `DaemonApiState`, and only then erase services into trait objects for generic supervision.

**Primary recommendation:** Build `PairingHost` and `PeerMonitor` as typed services in the composition root, pass clones into both `DaemonApiState` and `Vec<Arc<dyn DaemonService>>`, and make `DaemonApp` supervise only generic services.

## Standard Stack

### Core

| Library      | Version         | Purpose                                                | Why Standard                                                                                 |
| ------------ | --------------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| `uc-daemon`  | `0.4.0-alpha.1` | Daemon lifecycle, HTTP/WS/RPC host crate               | This phase refactors existing daemon structure, not crate selection                          |
| `uc-app`     | `0.1.0`         | Pairing/setup/space-access orchestrators and use cases | Existing business logic already lives here and should remain unchanged                       |
| `tokio`      | `1.x`           | Async runtime, `JoinSet`, channels, task spawning      | Already used across daemon lifecycle and officially supports the needed supervision patterns |
| `tokio-util` | `0.7`           | `CancellationToken` for coordinated shutdown           | Already established in daemon and officially supports parent/child cancellation propagation  |
| `axum`       | `0.7`           | Daemon HTTP + websocket server                         | Existing daemon API already depends on it and this phase should preserve behavior            |

### Supporting

| Library                  | Version                | Purpose                                                     | When to Use                                                                         |
| ------------------------ | ---------------------- | ----------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `async-trait`            | `0.1`                  | Async trait methods on `DaemonService`                      | Keep for `start()` / `stop()` on trait objects                                      |
| `tracing`                | `0.1`                  | Structured lifecycle logs for startup/shutdown/failures     | Use at service boundaries and unexpected exits                                      |
| `tokio::sync::broadcast` | bundled in `tokio 1.x` | Independent `NetworkEvent` subscribers and websocket fanout | Use for `PeerMonitor` + `PairingHost` concurrent subscriptions and daemon WS topics |

### Alternatives Considered

| Instead of                                              | Could Use                        | Tradeoff                                                                                                  |
| ------------------------------------------------------- | -------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Independent `subscribe_events()` per service            | Central event router/dispatcher  | More plumbing, more shared state, and unnecessary because broadcast already supports multiple subscribers |
| `Vec<Arc<dyn DaemonService>>` supervision               | Hardcoded fields per component   | Easier typed access, but every new daemon service reintroduces app boilerplate                            |
| Separate `PeerMonitor` + existing `PeerDiscoveryWorker` | Merge both into one peer service | Fewer files, but mixes websocket projection with network-start/announce side effects in the same phase    |

**Installation:**

```bash
# No new dependencies recommended for this phase.
```

**Version verification:** Verified locally on 2026-03-23 from `src-tauri/crates/uc-daemon/Cargo.toml`, `src-tauri/Cargo.toml`, and `cargo metadata --format-version 1 --no-deps`. Recommendation is to reuse the existing workspace stack rather than introduce new libraries.

## Architecture Patterns

### Recommended Project Structure

```text
src-tauri/crates/uc-daemon/src/
├── app.rs                 # Generic daemon supervisor only
├── service.rs             # DaemonService trait + ServiceHealth
├── state.rs               # RuntimeState + DaemonServiceSnapshot
├── pairing/
│   └── host.rs            # Pairing-only lifecycle and protocol handling
├── peers/
│   └── monitor.rs         # Peer lifecycle -> websocket projection
└── workers/
    ├── clipboard_watcher.rs
    └── peer_discovery.rs
```

### Pattern 1: Compose Typed Services Before Trait-Object Erasure

**What:** Construct `Arc<DaemonPairingHost>` and `Arc<PeerMonitor>` as typed values first, then clone them into `DaemonApiState` and into `Vec<Arc<dyn DaemonService>>`.
**When to use:** Always. This is the clean way to satisfy D-05/D-07 while preserving typed pairing APIs.
**Example:**

```rust
// Source: src-tauri/crates/uc-daemon/src/main.rs (recommended adaptation)
let pairing_host = Arc::new(DaemonPairingHost::new(...));
let peer_monitor = Arc::new(PeerMonitor::new(...));

let api_state = base_api_state.with_pairing_host(Arc::clone(&pairing_host));

let services: Vec<Arc<dyn DaemonService>> = vec![
    Arc::clone(&pairing_host) as Arc<dyn DaemonService>,
    Arc::clone(&peer_monitor) as Arc<dyn DaemonService>,
    Arc::new(ClipboardWatcherWorker),
    Arc::new(PeerDiscoveryWorker::new(...)),
];
```

### Pattern 2: One Event Subscription Per Responsibility

**What:** `PeerMonitor` and `PairingHost` each call `network_events.subscribe_events()` and filter for their own event variants.
**When to use:** For any long-lived daemon service that reacts to a subset of `NetworkEvent`.
**Example:**

```rust
// Source: https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html
let (tx, mut rx1) = broadcast::channel(16);
let mut rx2 = tx.subscribe();

tx.send(10)?;
assert_eq!(rx1.recv().await?, 10);
assert_eq!(rx2.recv().await?, 10);
```

### Pattern 3: Parent Cancellation, Child Tokens Per Service

**What:** `DaemonApp` owns one root `CancellationToken` and gives each service a `child_token()`.
**When to use:** Service start/stop supervision and graceful shutdown.
**Example:**

```rust
// Source: https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html
let token = CancellationToken::new();
let child = token.child_token();

// Cancelling parent cancels child; cancelling child does not affect parent.
token.cancel();
```

### Pattern 4: Uniform Service Supervision With JoinSet

**What:** Spawn all daemon services into one `JoinSet`, wait for shutdown or first unexpected exit, then drain and stop in reverse order.
**When to use:** `DaemonApp::run()`.
**Example:**

```rust
// Source: https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html
let mut tasks = JoinSet::new();
for service in &services {
    let svc = Arc::clone(service);
    let cancel = root.child_token();
    tasks.spawn(async move { svc.start(cancel).await });
}
```

### Anti-Patterns to Avoid

- **Erasing `PairingHost` too early:** if only `Arc<dyn DaemonService>` remains, API routes cannot call pairing mutations without downcasting or hidden globals.
- **Moving peer websocket emission into `PeerDiscoveryWorker`:** that couples frontend projection to network-start side effects and weakens the responsibility split.
- **Keeping peer events inside `run_pairing_protocol_loop`:** that preserves the main architecture smell this phase exists to remove.
- **Replacing full peer snapshot emission with ad-hoc deltas:** existing websocket tests and pending PH51-02 direction both expect full snapshot semantics on `peers.changed`.
- **Dropping subscribe retry/backoff during extraction:** current pairing network loop already retries failed subscriptions; `PeerMonitor` should preserve that resilience.

## Don't Hand-Roll

| Problem                            | Don't Build                                                | Use Instead                                                                | Why                                                                       |
| ---------------------------------- | ---------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Multi-service network event fanout | Custom dispatcher/router layer                             | Existing `NetworkEventPort::subscribe_events()` + `tokio::sync::broadcast` | Broadcast already supports multiple subscribers and explicit lag handling |
| Graceful shutdown graph            | Custom bool flags / bespoke stop channels                  | `CancellationToken` root + `child_token()`                                 | Official semantics match parent-child service shutdown exactly            |
| Parallel service tracking          | Manual `JoinHandle` fields like `completed_pairing_handle` | One `JoinSet` for all services                                             | Avoids scaling boilerplate and keeps exit behavior uniform                |
| Peer list reconciliation           | Hand-maintained incremental peer cache in the daemon       | Reuse `get_p2p_peers_snapshot()` and emit full `peers.changed` payloads    | Preserves current websocket contract and matches existing tests           |

**Key insight:** This phase does not need new infrastructure. The daemon already has the right runtime primitives; the issue is inconsistent application.

## Common Pitfalls

### Pitfall 1: Losing Typed Pairing Access After the Service Refactor

**What goes wrong:** `DaemonApp` becomes generic, but API routes and query code still need `active_session_id()`, `accept_pairing()`, `reject_pairing()`, and setup-state synthesis.
**Why it happens:** Trait-object erasure happens before `DaemonApiState` is assembled.
**How to avoid:** Keep `Arc<DaemonPairingHost>` in the composition root and clone it into both `DaemonApiState` and the generic service list.
**Warning signs:** The plan starts talking about downcasting `dyn DaemonService`, storing pairing host globally, or making routes generic over service lookup.

### Pitfall 2: Re-splitting By File Instead of By Responsibility

**What goes wrong:** `PeerMonitor` exists, but pairing-only code and peer-only code are still mixed through shared helpers.
**Why it happens:** The old `run_pairing_network_event_loop` handled both concerns, so copy/paste extraction is tempting.
**How to avoid:** Split the mixed loop by event family first, then move only the peer arm plus peer-only helpers into `PeerMonitor`.
**Warning signs:** `PeerMonitor` imports pairing orchestrator, or `PairingHost` still imports peer websocket DTO payloads.

### Pitfall 3: Hidden Lifecycle Still Lives In `DaemonApp`

**What goes wrong:** Services move into `Vec<Arc<dyn DaemonService>>`, but `DaemonApp::run()` still has special cases for pairing-host startup/shutdown.
**Why it happens:** Refactor stops at trait renaming.
**How to avoid:** Treat every long-lived daemon component in scope as a service and supervise them through the same start/join/stop path.
**Warning signs:** New code still contains `completed_xxx_handle` or one-off `tokio::spawn` blocks for a specific service.

### Pitfall 4: Breaking Subscription Resilience During Extraction

**What goes wrong:** `PeerMonitor` or the renamed pairing loop exits permanently after one failed `subscribe_events()` call.
**Why it happens:** Existing backoff logic is tied to the old mixed loop and gets lost during extraction.
**How to avoid:** Preserve retry/backoff logic in every long-lived subscriber that depends on `subscribe_events()`.
**Warning signs:** No backoff helper reuse and no test for receiver-close/retry behavior.

### Pitfall 5: Rename Drift Leaves Status/RPC Terms Half-Updated

**What goes wrong:** `service.rs` exists, but `DaemonWorkerSnapshot`, `WorkerHealth`, docs, and RPC mapping still use worker terms inconsistently.
**Why it happens:** Renames span `app.rs`, `state.rs`, `rpc/handler.rs`, `api/query.rs`, `main.rs`, and `lib.rs`.
**How to avoid:** Plan one explicit rename sweep and one explicit behavior sweep instead of mixing both everywhere.
**Warning signs:** New trait is `DaemonService`, but status/logging internals still say worker.

## Code Examples

Verified patterns from official sources and current codebase:

### Independent Broadcast Subscribers

```rust
// Source: https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html
let (tx, mut rx1) = broadcast::channel(16);
let mut rx2 = tx.subscribe();

tx.send(10)?;
assert_eq!(rx1.recv().await?, 10);
assert_eq!(rx2.recv().await?, 10);
```

### Service Trait Shape For This Phase

```rust
// Source: src-tauri/crates/uc-daemon/src/worker.rs (rename only)
#[async_trait]
pub trait DaemonService: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self, cancel: CancellationToken) -> anyhow::Result<()>;
    async fn stop(&self) -> anyhow::Result<()>;
    fn health_check(&self) -> ServiceHealth;
}
```

### PeerMonitor Event Filtering Skeleton

```rust
// Source: src-tauri/crates/uc-daemon/src/pairing/host.rs (peer arms extracted)
loop {
    tokio::select! {
        _ = cancel.cancelled() => return Ok(()),
        maybe_event = event_rx.recv() => {
            let Some(event) = maybe_event else { break; };
            match event {
                NetworkEvent::PeerDiscovered(_) | NetworkEvent::PeerLost(_) => {
                    emit_full_peer_snapshot(...).await?;
                }
                NetworkEvent::PeerConnected(peer) => emit_connection_changed(...),
                NetworkEvent::PeerDisconnected(peer_id) => emit_disconnected(...),
                NetworkEvent::PeerNameUpdated { peer_id, device_name } => emit_name_updated(...),
                _ => {}
            }
        }
    }
}
```

### Uniform DaemonApp Supervision

```rust
// Source: src-tauri/crates/uc-daemon/src/app.rs (recommended adaptation)
let mut tasks = JoinSet::new();
for service in &self.services {
    let svc = Arc::clone(service);
    let token = self.cancel.child_token();
    tasks.spawn(async move { svc.start(token).await });
}

tokio::select! {
    _ = wait_for_shutdown_signal() => {}
    Some(result) = tasks.join_next() => { /* unexpected exit handling */ }
}
```

## State of the Art

| Old Approach                                                          | Current Approach                                                                  | When Changed       | Impact                                                   |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------ | -------------------------------------------------------- |
| `run_pairing_network_event_loop` handles both peer and pairing events | `PeerMonitor` handles peer lifecycle; `PairingHost` handles pairing protocol only | Phase 56 (planned) | Clear ownership and easier tests                         |
| `DaemonApp` supervises workers generically but pairing host specially | `DaemonApp` supervises one `Vec<Arc<dyn DaemonService>>`                          | Phase 56 (planned) | New services do not require app boilerplate changes      |
| Pairing construction state stored in `DaemonApp`                      | Pairing construction happens before `DaemonApp::new()`                            | Phase 56 (planned) | Cleaner app boundary and better composition-root control |
| `DaemonWorker` naming in trait/file/state                             | `DaemonService` naming across daemon lifecycle                                    | Phase 56 (planned) | Internal terminology matches real responsibility         |

**Deprecated/outdated:**

- `src-tauri/crates/uc-daemon/src/worker.rs`: outdated after service unification; replace with `service.rs`
- `run_pairing_network_event_loop`: outdated name and outdated responsibility mix
- Pairing-specific constructor fields on `DaemonApp`: outdated once `PairingHost` is assembled before app creation

## Open Questions

1. **Where should `DaemonApiState` be assembled after D-07?**
   - What we know: `DaemonApiState` still needs a typed pairing host for routes and setup-state queries, while D-07 removes pairing-specific fields from `DaemonApp`.
   - What's unclear: whether `DaemonApp` should still receive a prebuilt API-state aggregate, or whether API assembly should move fully into `main.rs` / bootstrap.
   - Recommendation: keep typed assembly in the composition root and avoid service downcasting; planner should settle the exact constructor boundary early.

2. **Should `PeerDiscoveryWorker` merge into `PeerMonitor` now?**
   - What we know: `PeerDiscoveryWorker` currently starts the network and announces device name on `PeerDiscovered`; `PeerMonitor` will also subscribe to peer events.
   - What's unclear: whether the team wants one peer-domain service or two smaller services in this phase.
   - Recommendation: keep them separate in Phase 56. The goal here is clarity and lifecycle unification, not behavior consolidation.

3. **Should `WorkerHealth` also be renamed to `ServiceHealth`?**
   - What we know: D-06 explicitly renames the trait/file; `RuntimeState` and query DTO mapping still refer to worker status.
   - What's unclear: whether the team wants minimal churn or full terminology consistency in one pass.
   - Recommendation: rename it now inside `uc-daemon`; it is internal and aligns snapshots, state, and logs with the new abstraction.

## Validation Architecture

### Test Framework

| Property           | Value                                                                                                     |
| ------------------ | --------------------------------------------------------------------------------------------------------- |
| Framework          | Rust `cargo test` (`tokio::test`, crate integration tests, axum/tower HTTP/WS tests)                      |
| Config file        | `src-tauri/Cargo.toml` and `src-tauri/crates/uc-daemon/Cargo.toml`                                        |
| Quick run command  | `cd src-tauri && cargo test -p uc-daemon peer_discovered_emits_peers_changed_full_payload_with_peer_list` |
| Full suite command | `cd src-tauri && cargo test -p uc-daemon -- --test-threads=1`                                             |

### Phase Requirements → Test Map

| Req ID | Behavior                                                                                 | Test Type        | Automated Command                                                                                                                         | File Exists?          |
| ------ | ---------------------------------------------------------------------------------------- | ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | --------------------- |
| P56-A  | Peer lifecycle websocket projection is owned by `PeerMonitor`, not `PairingHost`         | unit             | `cd src-tauri && cargo test -p uc-daemon peer_discovered_emits_peers_changed_full_payload_with_peer_list`                                 | ✅                    |
| P56-B  | Generic service supervision starts/stops all in-scope daemon services uniformly          | unit/integration | `cd src-tauri && cargo test -p uc-daemon --lib app::tests`                                                                                | ✅ partial            |
| P56-C  | Pairing mutations still work through typed pairing-host access after service unification | integration      | `cd src-tauri && cargo test -p uc-daemon --test pairing_host -- --test-threads=1`                                                         | ✅ baseline not green |
| P56-D  | Peer websocket bridge contract remains unchanged after extraction                        | integration      | `cd src-tauri && cargo test -p uc-daemon --test pairing_ws peers_and_paired_devices_incremental_events_preserve_bridge_fields -- --exact` | ✅                    |
| P56-E  | New peer subscriber keeps retry/backoff and cancellation behavior                        | unit             | `cd src-tauri && cargo test -p uc-daemon peer_monitor_`                                                                                   | ❌ Wave 0             |

### Sampling Rate

- **Per task commit:** `cd src-tauri && cargo test -p uc-daemon peer_discovered_emits_peers_changed_full_payload_with_peer_list`
- **Per wave merge:** `cd src-tauri && cargo test -p uc-daemon -- --test-threads=1`
- **Phase gate:** Full `uc-daemon` suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `src-tauri/crates/uc-daemon/src/peers/monitor.rs` tests for peer-event filtering, snapshot emission, retry/backoff, and cancellation
- [ ] `src-tauri/crates/uc-daemon/src/app.rs` or `src-tauri/crates/uc-daemon/tests/service_lifecycle.rs` coverage for uniform service startup/shutdown and unexpected service exit
- [ ] Rename-consistency assertions for `DaemonServiceSnapshot` / `ServiceHealth` status mapping if terminology changes in this phase
- [ ] Current baseline issue: `cd src-tauri && cargo test -p uc-daemon --test pairing_host -- --test-threads=1` currently fails on `daemon_pairing_host_accept_pairing_projects_verifying_stage`; planner should not assume that suite is green before Phase 56 starts

## Sources

### Primary (HIGH confidence)

- `/websites/rs_tokio` - `tokio::sync::broadcast` subscriber semantics and `JoinSet` supervision patterns
- `/websites/rs_tokio-util` - `CancellationToken` child-token propagation and cancel-safe shutdown primitives
- `https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html` - verified multi-subscriber behavior and lag semantics
- `https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html` - verified task supervision pattern
- `https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html` - verified parent/child cancellation semantics
- `src-tauri/crates/uc-daemon/src/app.rs` - current daemon supervision and hardcoded pairing-host lifecycle
- `src-tauri/crates/uc-daemon/src/pairing/host.rs` - current responsibility mix and event-loop split target
- `src-tauri/crates/uc-daemon/src/workers/peer_discovery.rs` - current peer announce behavior and overlap boundary
- `src-tauri/crates/uc-daemon/src/state.rs` - current runtime snapshot naming and status shape
- `src-tauri/crates/uc-daemon/src/api/query.rs` - typed pairing-host dependency in setup/query paths
- `src-tauri/crates/uc-daemon/tests/pairing_host.rs` - existing pairing-host regression coverage
- `src-tauri/crates/uc-daemon/tests/pairing_ws.rs` - websocket contract coverage for peers and pairing

### Secondary (MEDIUM confidence)

- `src-tauri/crates/uc-daemon/src/main.rs` - current composition root and likely refactor insertion point
- `src-tauri/crates/uc-daemon/src/api/server.rs` - current API-state assembly shape
- `src-tauri/crates/uc-daemon/Cargo.toml` and `src-tauri/Cargo.toml` - local dependency/version verification

### Tertiary (LOW confidence)

- None

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH - existing workspace stack was verified locally and no new libraries are required
- Architecture: MEDIUM - the service split is clear, but the exact `DaemonApiState` assembly boundary still needs a planning decision
- Pitfalls: HIGH - risks are directly visible in current code structure and existing tests

**Research date:** 2026-03-23
**Valid until:** 2026-04-22
