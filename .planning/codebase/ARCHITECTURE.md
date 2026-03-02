# Architecture

**Analysis Date:** 2026-03-02

## Pattern Overview

**Overall:** Hexagonal Architecture (Ports and Adapters) with message-driven event runtime

**Key Characteristics:**

- Domain-driven design with clear separation between core domain, infrastructure, and platform adapters
- Trait-based port/adapter pattern for all external dependencies
- Event-driven async runtime using tokio channels for inter-component communication
- Crate-based module boundaries (uc-core, uc-infra, uc-platform, uc-app) enforce dependency direction
- Frontend-backend communication via Tauri commands and event subscriptions
- No global mutable state; all state accessed through ports

## Layers

**Domain Layer (uc-core):**

- Purpose: Pure business domain models with no external dependencies
- Location: `src-tauri/crates/uc-core/src/`
- Contains: Clipboard, Device, Network, Security, Setup, and Blob domain models; trait port definitions
- Depends on: Nothing (zero dependencies on other crates)
- Used by: uc-app (use cases), uc-infra (implementations), uc-platform (adapters)
- Key modules: `clipboard/`, `device/`, `network/`, `security/`, `ports/`

**Application Layer (uc-app):**

- Purpose: Use cases orchestrating domain models and coordinating with ports
- Location: `src-tauri/crates/uc-app/src/usecases/`
- Contains: Clipboard operations, encryption, settings, pairing, setup, space access workflows
- Depends on: uc-core ports; receives implementations via dependency injection
- Used by: uc-tauri commands, background tasks
- Key pattern: Pure use case classes with `execute()` methods taking ports as constructor parameters
- Examples: `ListClipboardEntries`, `DeleteClipboardEntry`, `CaptureClipboard`, `JoinSpace`

**Infrastructure Layer (uc-infra):**

- Purpose: Implement ports defined in uc-core; handle database, encryption, file system, network abstractions
- Location: `src-tauri/crates/uc-infra/src/`
- Contains: Database repositories (Diesel ORM), encryption (XChaCha20-Poly1305), file system (key slots, blobs), settings
- Depends on: uc-core (ports only)
- Used by: uc-tauri bootstrap to inject implementations
- Key modules: `db/repositories/`, `security/encryption.rs`, `fs/`, `settings/`

**Platform Layer (uc-platform):**

- Purpose: Bridge between Tauri, OS-specific features, and application core
- Location: `src-tauri/crates/uc-platform/src/`
- Contains: Clipboard watcher, app directory resolution, IPC event/command bus, pairing stream framing
- Depends on: uc-core (ports only, not implementations)
- Used by: uc-tauri main.rs to initialize PlatformRuntime
- Key modules: `runtime/`, `clipboard/`, `ipc/`, `adapters/`

**Tauri Adapter Layer (uc-tauri):**

- Purpose: Wire dependencies, register Tauri commands, coordinate startup sequence
- Location: `src-tauri/crates/uc-tauri/src/`
- Contains: Command handlers, bootstrap logic, event adapters, models (DTOs), protocols
- Depends on: uc-app (use cases), uc-infra (implementations), uc-core (models), uc-platform (runtime)
- Used by: src-tauri/src/main.rs
- Key files: `bootstrap/`, `commands/`, `services/`, `events/`

**Frontend Layer (React + TypeScript):**

- Purpose: User interface with state management and API integration
- Location: `src/`
- Contains: Pages, components (UI), layouts, hooks, Redux Toolkit store, Tauri API wrappers
- Depends on: Tauri commands (uc-tauri), event listeners
- Key modules: `pages/`, `components/`, `store/`, `api/`, `hooks/`

## Data Flow

**Clipboard Capture (System → Database → Frontend):**

1. `PlatformRuntime` watches system clipboard changes (clipboard_rs crate)
2. ClipboardWatcher detects change, emits `PlatformEvent::ClipboardChanged { snapshot }`
3. PlatformRuntime dispatches event to `ClipboardChangeHandler` callback (trait in uc-core)
4. `AppRuntime` implements callback, invokes `CaptureClipboard` use case
5. UseCase: persists clipboard event, creates representations, saves blobs, creates ClipboardEntry
6. ClipboardEntryRepo saves entry to SQLite database
7. Frontend polls `get_clipboard_entries` command to fetch list of entries
8. Frontend renders preview images (fetched via `uc://blob/<blob_id>` protocol)

**Tauri Command Execution Flow:**

1. Frontend invokes Tauri command (e.g., `get_clipboard_entries`)
2. Command handler in `src-tauri/crates/uc-tauri/src/commands/` receives `State<'_, Arc<AppRuntime>>`
3. Handler calls `runtime.usecases().desired_use_case()` to get use case instance
4. UseCases accessor wires ports into use case constructor (uc-app pattern)
5. UseCase executes, reading/writing through ports
6. Infrastructure implementations (uc-infra) handle actual I/O (database, filesystem)
7. Result serialized and returned to frontend as TypeScript DTO

**Setup Workflow (P2P Pairing → Space Access → Device Sync):**

1. Frontend calls `start_join_space` → `SetupOrchestrator` (cached in AppRuntime)
2. Setup orchestrator: prompts for passphrase → peer discovery via libp2p
3. `PairingOrchestrator` handles peer selection and HMAC verification via pairing stream framing
4. Once paired, `SpaceAccessOrchestrator` derives shared key from passphrase
5. Backend creates Device, saves to database
6. Frontend polls `get_setup_state` to drive UI through wizard steps
7. On completion, encryption is initialized with derived key

**Encryption Session Lifecycle:**

1. AppStartup: load encryption state from secure storage (Stronghold)
2. If initialized but session not ready: redirect to unlock page
3. Frontend submits passphrase via `unlock_encryption_session` command
4. Backend derives key from passphrase (Argon2id), unlocks secure storage
5. Encryption session marked ready; backend broadcasts `encryption://event` with `SessionReady`
6. Frontend receives event, can now decrypt clipboard entries

**State Management:**

- **Backend:** No global state; all state accessed through ports
  - Encryption state: `EncryptionState` port in uc-infra (thread-safe via Mutex/RwLock)
  - Settings: `SettingsPort` (TOML file with RwLock wrapper)
  - Database: Diesel connection pool (Arc-wrapped)
  - Lifecycle status: `LifecycleStatusPort` (in-memory with interior mutability)

- **Frontend:** Redux Toolkit store with slices
  - `appApi` (RTK Query): queries for clipboard entries, encryption status, settings
  - `clipboardSlice`: pagination state, selected entry
  - `devicesSlice`: paired devices list
  - `statsSlice`: app statistics

## Key Abstractions

**Port (Interface) Pattern:**

Defined in `src-tauri/crates/uc-core/src/ports/`:

- `ClipboardEntryRepository` - CRUD operations for clipboard entries
- `ClipboardEventRepository` - Event log for clipboard changes
- `DeviceRepository` - Paired devices storage
- `SettingsPort` - Application settings persistence
- `SystemClipboardPort` - OS clipboard read/write
- `BlobStore` - Large binary data storage (images, files)
- `ThumbnailRepository` - Image thumbnail caching
- `EncryptionSession` - Crypto key management and encryption/decryption
- `SecurityCrypto` - Cryptographic primitives (XChaCha20-Poly1305)
- `TimerPort` - Async timing utilities
- `ClipboardChangeHandler` - Callback for clipboard changes (app implements)
- `SpaceAccessTransportPort` - Network transport for space join flow
- `SpaceAccessCryptoFactory` - Creates crypto operations for space access

**Adapter (Implementation) Pattern:**

Located in `src-tauri/crates/uc-infra/src/`:

- `DieselClipboardEntryRepo` implements `ClipboardEntryRepository`
- `DieselDeviceRepo` implements `DeviceRepository`
- `JsonKeySlotStore` implements `KeySlotStore` (key material persistence)
- `XChaCha20Encryption` implements `EncryptionSession`
- `LocalClipboard` implements `SystemClipboardPort` (platform-specific via clipboard_rs)

Located in `src-tauri/crates/uc-platform/src/`:

- `PlatformRuntime` - Main event loop and clipboard watcher integration
- `ClipboardWatcher` - Wraps clipboard_rs for change detection
- `PairingStreamFramer` - Encodes/decodes messages for pairing flow
- `LocalClipboard` - Clipboard API adapter (wraps clipboard_rs)

**UseCase Pattern:**

Located in `src-tauri/crates/uc-app/src/usecases/`:

```rust
// Example: GetEntryDetail use case
pub struct GetClipboardEntryDetail {
    clipboard_entry_repo: Arc<dyn ClipboardEntryRepository>,
    representation_repo: Arc<dyn RepresentationRepository>,
}

impl GetClipboardEntryDetail {
    pub fn new(
        clipboard_entry_repo: Arc<dyn ClipboardEntryRepository>,
        representation_repo: Arc<dyn RepresentationRepository>,
    ) -> Self {
        Self { clipboard_entry_repo, representation_repo }
    }

    pub async fn execute(&self, entry_id: &EntryId) -> Result<EntryDetail> {
        let entry = self.clipboard_entry_repo.get_entry(entry_id).await?;
        let representations = self.representation_repo.get_by_entry(entry_id).await?;
        Ok(EntryDetail { entry, representations })
    }
}
```

Each use case:

- Takes only ports as dependencies (via constructor)
- Has a single public `execute()` method
- Returns Result with domain types
- No Tauri or command knowledge

## Entry Points

**Backend Entry:**

- Location: `src-tauri/src/main.rs`
- Triggers: Application startup
- Responsibilities:
  1. Initialize tracing subscriber (structured logging)
  2. Load configuration (development: config.toml, production: system defaults)
  3. Wire dependencies using `wire_dependencies()` from uc-tauri bootstrap
  4. Create channels for PlatformRuntime event/command communication
  5. Initialize Tauri Builder with plugins
  6. Register all Tauri command handlers via `invoke_handler![]`
  7. Spawn PlatformRuntime startup task in setup block
  8. Register URI protocol handler (`uc://`) for blob/thumbnail serving

**Command Entry Points:**

- Location: `src-tauri/crates/uc-tauri/src/commands/`
- Trigger: Frontend invokes Tauri command
- Pattern:
  ```rust
  #[tauri::command]
  pub async fn get_clipboard_entries(
      runtime: State<'_, Arc<AppRuntime>>,
      limit: Option<usize>,
  ) -> Result<Response, String> {
      let uc = runtime.usecases().list_entry_projections();
      let result = uc.execute(limit, 0).await?;
      Ok(Response { entries: result })
  }
  ```

**Frontend Entry:**

- Location: `src/main.tsx`
- Triggers: App load in browser
- Responsibilities:
  1. Initialize Sentry error tracking
  2. Initialize logging (attach Tauri console plugin)
  3. Set up Redux store with RTK Query
  4. Initialize i18n (internationalization)
  5. Apply platform-specific typography scaling (Windows smaller fonts)
  6. Render React root with context providers

**App Router Entry:**

- Location: `src/App.tsx`
- Triggers: After main.tsx ReactDOM render
- Responsibilities:
  1. Load setup state to check if setup wizard needed
  2. Check encryption session status (initialized, ready)
  3. Route to SetupPage, UnlockPage, or authenticated routes (Dashboard, Devices, Settings)
  4. Manage context providers (SearchProvider, ShortcutProvider, UpdateProvider)
  5. Handle event listeners for encryption and UI navigation

**PlatformRuntime Entry:**

- Location: `src-tauri/crates/uc-platform/src/runtime/runtime.rs`
- Triggers: `PlatformRuntime::start()` spawned in main.rs setup block
- Responsibilities:
  1. Start clipboard watcher (platform-specific via clipboard_rs)
  2. Listen on event channel from clipboard watcher
  3. Listen on command channel from Tauri commands
  4. Call `ClipboardChangeHandler` callback when clipboard changes
  5. Run async event loop until app shutdown

## Error Handling

**Strategy:** Explicit Result types with anyhow::Error in infrastructure; domain-specific errors in application

**Patterns:**

**Infrastructure Layer (uc-infra):**

- All fallible operations return `Result<T, anyhow::Error>`
- Database errors wrapped with context: `query().context("Failed to fetch entry")?`
- Encryption errors propagated as anyhow errors

**Application Layer (uc-app):**

- Use cases return domain error types (e.g., `EntryNotFound`, `DecryptionFailed`)
- Custom error enums implement `From<anyhow::Error>` for conversion from infrastructure
- Error messages include context for UI display

**Command Layer (uc-tauri):**

- Commands return `Result<T, String>` for Tauri compatibility
- Use case errors converted to strings: `uc.execute().map_err(|e| e.to_string())?`
- Errors logged to tracing before returning to frontend

**Frontend:**

- API queries wrapped with RTK Query error handling
- Try-catch blocks for async operations
- Errors displayed to user via toast notifications (Sonner)
- Fallback UI states for failed requests

## Cross-Cutting Concerns

**Logging:** Structured logging with tracing crate

- Backend: `src-tauri/crates/uc-tauri/src/bootstrap/tracing.rs` initializes subscriber
- Spans for operation tracking: `info_span!("operation.name", field = value)`
- `.instrument(span)` attaches spans to async blocks for duration tracking
- Development: Debug level to terminal; Production: Info level to file + stdout
- Filtered to exclude noisy libp2p_mdns, Tauri internal events

**Validation:** Input validation at command layer

- Parse and validate Tauri command inputs before passing to use cases
- IDs validated (e.g., EntryId, BlobId format checks)
- No unchecked user input passed to use cases

**Authentication:** Encryption session as gatekeeper

- Commands check `encryption_session.is_ready()` before accessing clipboard data
- Uninitialized state redirects to setup wizard
- Initialized but locked redirects to unlock page
- Session marked ready only after secure storage unlock completes

**Concurrency:** Tokio async runtime with Arc/Mutex for shared state

- Arc<dyn Port> for dependency injection
- Mutex<T> around mutable state in adapters (encryption state, settings cache)
- RwLock for read-heavy state (app handle, lifecycle status)
- Channel-based message passing between runtime components (no shared mutable state between them)

**Testing:** Infrastructure tested in isolation; use cases tested with mock ports

- `#[cfg(test)]` modules in each module with unit tests
- Integration tests in `tests/` directories
- Mocks created by implementing port traits
- No database tests (Diesel migrations in separate flow)

---

_Architecture analysis: 2026-03-02_
