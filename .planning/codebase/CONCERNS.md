# Codebase Concerns

**Analysis Date:** 2026-03-02

## Tech Debt

### Architecture Migration in Progress - Hexagonal vs Clean Architecture

**Issue:** The codebase is mid-migration from Clean Architecture (legacy in `src-tauri/src/`) to Hexagonal Architecture (new in `src-tauri/crates/`). Both patterns coexist, creating maintenance burden.

**Files:**

- `src-tauri/src/` (legacy, ~40% of backend)
- `src-tauri/crates/` (new architecture, ~60% of backend)

**Impact:**

- Code reviewers must understand two architectural patterns
- Dependency injection duplicated between patterns
- Risk of introducing architectural violations when adding features to wrong layer
- Some use cases bridge both patterns, creating tight coupling

**Fix approach:**

- Continue migration phase by phase (see `.sisyphus/plans/` for phased approach)
- Complete migration of remaining legacy code
- Retire old architecture directories once all functionality migrated

---

### Unused Dependency: `aes-gcm`

**Issue:** `aes-gcm` crate included in `src-tauri/Cargo.lock` but not actively used anywhere. Project uses `chacha20poly1305` for encryption instead.

**Files:** `src-tauri/Cargo.lock`

**Impact:**

- Adds unnecessary binary bloat (~200KB+ in release build)
- Increases dependency maintenance burden
- Creates confusion about encryption strategy

**Fix approach:**

- Remove `aes-gcm` from workspace dependencies
- Run `cargo update` to clean Cargo.lock
- Verify no hidden transitive dependencies pull it back

---

### Large Wiring Module - 5,048 Lines

**Issue:** `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs` is extremely large (5,048 lines) and contains all dependency injection setup, multiple test utilities, and build-time compile checks.

**Files:** `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs`

**Impact:**

- Difficult to navigate and maintain
- Slow compilation due to module size
- Test utils and production code mixed
- Contains integration tests in production module (disabled with `#[ignore]` note at lines 4862-4872)

**Fix approach:**

- Split into separate modules: `wiring/production.rs`, `wiring/tests.rs`, `wiring/checks.rs`
- Move integration tests to `src-tauri/tests/` directory with proper test isolation
- Create builder pattern utilities in separate module to reduce monolithic dependency construction

---

### Missing Property Implementations in Clipboard List

**Issue:** `is_encrypted` and `is_favorited` fields in clipboard entry projections hardcoded to `false` with "TODO: implement later" comments.

**Files:** `src-tauri/crates/uc-app/src/usecases/clipboard/list_entry_projections/list_entry_projections.rs` (lines 22, 191-192)

**Impact:**

- Frontend cannot show encryption or favorite status
- API contracts promise these fields but don't deliver meaningful values
- Feature incompleteness hidden from UX

**Fix approach:**

- Implement `is_encrypted` by checking if entry's representations have encryption metadata
- Implement `is_favorited` by adding favorite_flag to clipboard entry schema and repository
- Add database migration for favorite_flag column if needed

---

### Stub Implementations in Platform Layer

**Issue:** Platform adapter stubs with "TODO" comments not yet implemented:

**Files:**

- `src-tauri/crates/uc-platform/src/adapters/blob.rs:23` - blob writing stub
- `src-tauri/crates/uc-platform/src/adapters/ui.rs:10` - UI integration stub
- `src-tauri/crates/uc-platform/src/adapters/autostart.rs` - autostart enable/disable stubs (lines 9, 15, 23)

**Impact:**

- Blob storage not persisted to disk (only in-memory)
- UI integration features not wired (related to system notifications)
- Autostart functionality non-functional on macOS/Linux

**Fix approach:**

- Implement blob adapter using existing filesystem infrastructure in `src-tauri/crates/uc-platform/src/app_dirs.rs`
- Wire UI integration for toast notifications (Tauri plugin available)
- Implement autostart using `tauri-plugin-autostart` already in Cargo.toml

---

### Command Execution Stub - PlatformCommandExecutor

**Issue:** `src-tauri/src/main.rs:51-73` contains `SimplePlatformCommandExecutor` that logs commands but doesn't execute them.

**Files:** `src-tauri/src/main.rs` (lines 43-73)

**Impact:**

- Platform commands (StartClipboardWatcher, WriteClipboard, etc.) are no-ops
- Clipboard synchronization may not trigger correctly
- Silent failures if watcher doesn't actually start

**Fix approach:**

- Wire executor to actual platform implementations in `src-tauri/crates/uc-platform/`
- Implement proper command dispatch for each platform (macOS, Linux, Windows)
- Add integration tests for command execution

---

## Known Bugs

### Test Database Locking in Integration Tests

**Issue:** Integration test at `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs:4862-4872` is disabled because multiple parallel test runs lock the SQLite database.

**Files:** `src-tauri/crates/uc-tauri/src/bootstrap/wiring.rs`

**Status:** Currently ignored with explanation, but not fixed

**Impact:**

- Can't run full test suite in parallel
- Test isolation not guaranteed
- CI pipelines forced to run tests sequentially

**Workaround:** Tests disabled with `#[ignore]` attribute

**Fix approach:**

- Move integration tests to `src-tauri/tests/` directory with sequential runner
- Use unique temporary database paths per test (`TempDir` from `tempfile` crate)
- Or switch to in-memory SQLite database for test isolation
- Reference: `docs/architecture/commands-status.md` mentions this issue

---

### Missing LSP Tooling in CI Environment

**Issue:** Rust analyzer (`rust-analyzer`) not available in GitHub Actions official toolchain, preventing LSP-backed diagnostics.

**Files:** Affects all Rust source files during CI

**Impact:**

- Can't use automated static analysis in CI
- Code quality checks require manual verification
- Some subtle type system bugs might slip through

**Workaround:** Use `cargo check` and `cargo test` for verification instead

**Fix approach:**

- Add custom Rust toolchain installation step in CI workflow
- Or install `rust-analyzer` separately in actions
- Configure clippy lints in CI for lint-based checks

---

### PlatformClipboardPort Recursion Detection (Fragile)

**Issue:** `src-tauri/crates/uc-core/tests/platform_clipboard_port.rs:38-41` has comment indicating a BUG where blanket implementation can call itself recursively.

**Files:** `src-tauri/crates/uc-core/tests/platform_clipboard_port.rs`

**Impact:**

- Potential for infinite recursion in clipboard implementations
- Test only detects first level (counter >= 1), not deeper recursion
- Architecture violation: trait blanket impl calling trait method on self

**Fix approach:**

- Review `PlatformClipboardPort` blanket implementation
- Ensure it calls `SystemClipboardPort::read_snapshot()` not `self.read_snapshot()`
- Strengthen test to detect nested recursion

---

## Security Considerations

### Encryption State Initialization Detection

**Issue:** Comment in `src-tauri/crates/uc-infra/src/security/encryption_state.rs:45` indicates missing logic to identify "Initializing" state.

**Files:** `src-tauri/crates/uc-infra/src/security/encryption_state.rs`

**Risk:**

- Race condition possible: encryption might be queried during initialization
- User data could be accessed before encryption is fully initialized
- State machine doesn't properly track intermediate states

**Current mitigation:** Code is in infrastructure layer, protected by port abstraction

**Recommendations:**

- Complete state machine implementation to track all encryption lifecycle states
- Add explicit `Initializing` state to encryption state machine
- Document initialization sequence in `docs/architecture/bootstrap.md`

---

### Cryptographic Material Handling

**Issue:** Comments indicate security debt in `src-tauri/crates/uc-core/src/security/model.rs:206` and `264`:

- Line 206: "TODO: Remove Clone trait" - Clone enables unnecessary copies of encryption keys
- Line 264: "TODO: consider adding `zeroize` to wipe on drop" - Encrypted material not cleared from memory

**Files:** `src-tauri/crates/uc-core/src/security/model.rs`

**Risk:**

- Cloned encryption keys increase exposure window
- Memory containing keys not zeroized, possibly recoverable via memory forensics
- Violates cryptographic best practices

**Current mitigation:** Data structures use `SecretString` from `secrecy` crate

**Recommendations:**

- Remove or restrict `Clone` on key types (use `Arc<T>` or explicit copying only)
- Add `zeroize` crate dependency
- Implement `Zeroize` trait on key material types
- Document key lifecycle in architecture docs

---

### Missing HMAC-DRBG State Validation

**Issue:** `src-tauri/crates/uc-app/src/usecases/space_access/crypto_adapter.rs:611` and `:680` use "unused" placeholder secrets for HMAC computation in tests.

**Files:** `src-tauri/crates/uc-app/src/usecases/space_access/crypto_adapter.rs`

**Risk:**

- Test secrets leaking into production code paths
- HMAC validation can be bypassed if placeholder used in non-test paths
- Weakens space access authentication

**Fix approach:**

- Ensure "unused" secrets only appear in test mocks
- Add compile-time check to prevent placeholder secrets in production code
- Mock HMAC computation for deterministic testing instead of using placeholder secrets

---

## Performance Bottlenecks

### Large Async File Operations Without Chunking

**Issue:** `src-tauri/crates/uc-infra/src/clipboard/background_blob_worker.rs` and blob writing operations don't mention chunking for large files.

**Files:** `src-tauri/crates/uc-infra/src/clipboard/background_blob_worker.rs` (1,043 lines)

**Problem:**

- Large clipboard images (10MB+) loaded entirely into memory during write/read
- No streaming or chunked processing visible
- Potential OOM on low-memory systems

**Current capacity:** No documented limits for blob size

**Improvement path:**

- Implement streaming blob write with configurable chunk size (e.g., 1MB chunks)
- Add blob size validation before accepting clipboard content
- Document maximum clipboard content size in settings

---

### Network Command Queue Saturation

**Issue:** `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs:42` defines `MAX_IN_FLIGHT_BUSINESS_COMMANDS: usize = 16`.

**Files:** `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs`

**Problem:**

- Fixed queue size (16 commands) could be exhausted by rapid clipboard changes
- No backpressure or dropping strategy documented
- No metrics on queue depth or saturation events

**Current limit:** 16 commands maximum in-flight

**Scaling path:**

- Make queue depth configurable via settings
- Add telemetry for queue depth monitoring
- Implement exponential backoff for rejected commands
- Document expected throughput and queue sizing guidance

---

### Clipboard Representation Caching Strategy

**Issue:** `src-tauri/crates/uc-core/src/clipboard/policy/v1.rs` and representation cache in infrastructure don't show eviction strategy.

**Files:** Multiple representation repository files

**Problem:**

- Cache growth unbounded (no LRU, TTL, or size limits visible)
- Large clipboard histories could consume excessive memory
- No retention policy visible

**Improvement path:**

- Implement LRU eviction with configurable size limit
- Add TTL for representation cache entries
- Document cache retention policy
- Add metrics for cache hit rate and size

---

## Fragile Areas

### Setup State Machine - State Race Condition Prevention

**Issue:** `src-tauri/crates/uc-app/src/usecases/setup/context.rs:19` mentions "Serializes dispatch calls to prevent concurrent state/action races."

**Files:** `src-tauri/crates/uc-app/src/usecases/setup/context.rs`, `src-tauri/crates/uc-app/src/usecases/setup/orchestrator.rs`

**Why fragile:**

- Serialization mutex needed indicates tight coupling between state and actions
- Any concurrent dispatch attempt blocks entire setup flow
- If lock is held too long, UI can appear frozen

**Safe modification:**

- Acquire setup context lock, check state, release immediately
- Never hold lock across I/O operations
- Use immutable state snapshot for decision-making
- Test coverage: `src-tauri/crates/uc-app/tests/setup_flow_integration_test.rs`

---

### Pairing State Machine - 2,277 Lines

**Issue:** `src-tauri/crates/uc-core/src/network/pairing_state_machine.rs` is extremely large state machine with many transitions.

**Files:** `src-tauri/crates/uc-core/src/network/pairing_state_machine.rs` (2,277 lines)

**Why fragile:**

- State transitions not trivial to verify
- Many edge cases (timeout, network failure, user cancel during different states)
- Difficult to comprehend full state space

**Safe modification:**

- Understand existing state transition diagram before modifying
- Add integration tests for new transitions (`src-tauri/crates/uc-app/tests/` contains pairing tests)
- Verify invariants: no invalid state combinations, no unreachable states
- Test coverage: `src-tauri/crates/uc-app/src/usecases/pairing/orchestrator.rs` (2,088 lines) orchestrates tests

---

### libp2p Network Adapter - 2,927 Lines

**Issue:** `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs` is large network adapter with peer management, caching, and streaming logic.

**Files:** `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs` (2,927 lines)

**Why fragile:**

- Peer state cache (`PeerCaches` struct) mutable and shared
- Multiple concurrent operations (discovery, connection, pairing) interleave
- Race conditions possible between cache updates and network events

**Safe modification:**

- Don't modify peer cache from multiple async tasks without synchronization
- Prefer immutable snapshots when passing peer state to orchestrators
- Review atomic operations on state flags (`AtomicU8` used for START_STATE)
- Test coverage: Unit tests at end of file, integration via platform tests

---

### Encryption Session Management

**Issue:** `src-tauri/crates/uc-infra/src/security/encryption_session.rs` manages encryption state lifecycle.

**Files:** `src-tauri/crates/uc-infra/src/security/encryption_session.rs`

**Why fragile:**

- Session can expire mid-operation
- Multiple ports depend on encryption state consistency
- State transitions not synchronized with clipboard operations

**Safe modification:**

- Check encryption session validity before operations
- Handle expired session gracefully (restart encryption)
- Document session lifetime and refresh semantics
- Test: `src-tauri/crates/uc-app/src/usecases/auto_unlock_encryption_session.rs` handles session recovery

---

## Test Coverage Gaps

### Untested Area: Clipboard Content Type Detection

**Issue:** `src/api/clipboardItems.ts:166` has comment "Currently treating all entries as text. Implement proper content type detection"

**Files:** `src/api/clipboardItems.ts`

**Risk:**

- Image clipboard content returns incorrect MIME type
- Frontend doesn't know if content is text, image, HTML, or other
- Download/preview features broken for non-text content

**What's not tested:**

- Multi-format clipboard entries (both text and image)
- MIME type detection from database representations
- Content type validation in round-trip sync

**Priority:** High (blocks image sync)

---

### Untested Area: Frontend Image Dimension Handling

**Issue:** `src/api/clipboardItems.ts:176` has comment "TODO: 使用原图的宽高信息" (Use original image dimensions)

**Files:** `src/api/clipboardItems.ts`

**Risk:**

- Thumbnail dimensions hardcoded instead of using actual image dimensions
- Preview layout incorrect for non-square images
- Image aspect ratio not preserved

**What's not tested:**

- Image metadata extraction from clipboard
- Dimension information roundtrip through sync
- Thumbnail generation respecting original aspect ratio

**Priority:** Medium (affects UI presentation)

---

### Untested Area: Settings Sync Frequency

**Issue:** `src/components/setting/SyncSection.tsx:55` comment indicates backend only supports 'realtime' and 'interval' but frontend might offer more options.

**Files:** `src/components/setting/SyncSection.tsx`

**Risk:**

- User selects unsupported sync frequency
- Silent failure or incorrect sync behavior
- Settings state mismatch between frontend and backend

**What's not tested:**

- All sync frequency enum values acceptance
- Behavior when unsupported frequency selected
- Settings persistence validation

**Priority:** Medium (affects sync behavior)

---

### Missing Frontend Tests

**Issue:** Only 25 frontend test files found in `src/` directory. Component testing is minimal.

**Files:** Throughout `src/`

**Risk:**

- UI regressions not caught
- State management changes break without warning
- Cross-browser compatibility issues

**Improvement path:**

- Add Vitest configuration (deps already available)
- Test critical flows: setup, pairing, encryption initialization
- Mock Tauri command invocations
- Test Redux store slices for clipboard/device state

---

### Integration Test Isolation Issues

**Issue:** Tests in `src-tauri/crates/uc-app/tests/` share database and may interfere with each other.

**Files:** `src-tauri/crates/uc-app/tests/*.rs`

**Example:** `src-tauri/crates/uc-app/tests/snapshot_cache_integration_test.rs:758` has "TODO(Task 14): Add SpoolScanner and recovery assertions."

**Risk:**

- Test failures non-deterministic (depend on execution order)
- Difficult to debug test-specific issues
- Can't run tests in parallel

**Improvement path:**

- Use unique temporary databases per test
- Or configure serial test execution
- Add test fixtures for common setup
- Clear data between test cases

---

## Scaling Limits

### Database Connection Pool Size

**Issue:** Connection pool configuration not visible in source. libsqlite3-sys used which has thread-local storage limits.

**Files:** `src-tauri/crates/uc-infra/src/db/pool.rs`

**Current capacity:** Not documented

**Limit:** SQLite itself is single-writer (locks on write), so pool size doesn't increase write throughput

**Scaling path:**

- Document max concurrent read connections
- Implement connection retry logic for pool exhaustion
- Consider PostgreSQL for truly concurrent write workloads if scaling becomes needed
- Add metrics for pool utilization

---

### Clipboard Spool Queue Depth

**Issue:** `src-tauri/crates/uc-infra/src/clipboard/spool_queue.rs` and janitor may not handle high-frequency clipboard changes.

**Files:** `src-tauri/crates/uc-infra/src/clipboard/spool_queue.rs`, `src-tauri/crates/uc-infra/src/clipboard/spool_janitor.rs`

**Current capacity:** Not documented, queue size unspecified

**Limit:** Rapid clipboard changes (e.g., from automation tools) could overwhelm queue

**Scaling path:**

- Document queue depth limits
- Implement bounded queue with drop/backpressure strategy
- Add metrics for queue depth and processing latency
- Test with high-frequency clipboard simulator

---

### Network Peer Discovery Scalability

**Issue:** `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs` uses `HashMap` for peer caches without documented limits.

**Files:** `src-tauri/crates/uc-platform/src/adapters/libp2p_network.rs` (lines 69-72)

**Current capacity:** Unlimited (LAN-only, so small network assumed)

**Limit:** 1000+ discovered peers could cause:

- Memory bloat
- Slow lookups and iteration
- UI lag when displaying peer lists

**Scaling path:**

- Implement peer cache pruning (remove stale entries)
- Add TTL for discovered peer entries
- Limit cache size with LRU eviction
- Document expected peer count assumptions

---

## Dependencies at Risk

### Update Check Error Handling

**Issue:** Recent fix at commit `bc2cd4f9` addressed "invalid HTML nesting in AlertDialogDescription", suggesting updater integration has fragile DOM assumptions.

**Files:** Related to dialog rendering for update notifications

**Risk:**

- Update notifications could fail silently
- User never notified of critical security updates
- Desktop app remains vulnerable

**Migration plan:**

- Review entire update dialog flow for other HTML nesting issues
- Consider using Shadcn Dialog components for consistent HTML structure
- Add integration tests for update notification rendering

---

### Artifact Naming in Release Workflow

**Issue:** Recent commits `bc2cd4f9`, `219d6f2b`, `7cd247fd` all fix updater artifact naming issues.

**Files:** Release workflow configuration, updater service integration

**Risk:**

- Release artifacts not found by updater
- Auto-update feature fails silently
- Users stuck on old versions

**Current state:** Recently fixed (commit `219d6f2b`), but naming mismatch recurred

**Migration plan:**

- Document exact artifact naming convention
- Add validation in CI to verify artifact names match updater expectations
- Consider using generated manifest with hardcoded paths to prevent naming drift

---

### HTML Structure Fragility

**Issue:** Multiple DOM nesting issues fixed (commit `bc2cd4f9`). AlertDialog uses strict HTML validation.

**Files:** React components using Shadcn UI

**Risk:**

- Small layout changes break semantic HTML
- Screen reader compatibility issues
- Future Shadcn/React upgrades could introduce more nesting violations

**Migration plan:**

- Review all DialogContent usage for proper semantic structure
- Add HTML validation lint rule to eslint config
- Test with accessibility tools (axe, WAVE)

---

## Missing Critical Features

### Clipboard Content Encryption Status Display

**Issue:** `is_encrypted` field in clipboard list entry projections not implemented (lines 191-192 in list_entry_projections.rs).

**Blocks:**

- Users can't verify clipboard content is encrypted
- No visual indication of which entries have encryption
- Compliance/audit features impossible

---

### Clipboard Entry Favorite Marking

**Issue:** `is_favorited` field not implemented (lines 191-192 in list_entry_projections.rs).

**Blocks:**

- Users can't mark important clipboard entries for quick access
- Pinning/star feature not available
- History browsing harder for frequently-used content

---

### Autostart on System Boot

**Issue:** Platform adapters for autostart stubbed but not implemented.

**Blocks:**

- App doesn't launch on boot
- Background sync disabled after restart
- Users must manually launch for clipboard sync

---

### Network Watcher State Tracking

**Issue:** Comment at `src-tauri/crates/uc-core/src/setup/state_machine.rs:343` indicates "Setup state machine itself has no explicit Ready state."

**Blocks:**

- Can't determine when to start network after setup completes
- Network startup timing ambiguous
- Related to `start_network_after_unlock` use case issues

---

## Summary Table

| Issue Type       | Count | Severity    | Examples                                                       |
| ---------------- | ----- | ----------- | -------------------------------------------------------------- |
| Tech Debt        | 6     | Medium-High | Architecture migration, large modules, stub implementations    |
| Known Bugs       | 3     | Medium      | Test isolation, LSP tooling, recursion detection               |
| Security         | 3     | Medium      | Crypto handling, state initialization, HMAC validation         |
| Performance      | 4     | Low-Medium  | Large file handling, queue saturation, cache growth            |
| Fragile Areas    | 5     | Medium      | State machines, network adapters, encryption sessions          |
| Test Gaps        | 4     | Medium-High | Content types, settings, frontend tests, integration isolation |
| Scaling Limits   | 3     | Low-Medium  | Database, clipboard queue, peer discovery                      |
| Dependencies     | 3     | Medium      | Update artifacts, HTML structure, unused imports               |
| Missing Features | 4     | Medium      | Encryption status, favorites, autostart, network state         |

---

_Concerns audit: 2026-03-02_
