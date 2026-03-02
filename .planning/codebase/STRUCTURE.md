# Codebase Structure

**Analysis Date:** 2026-03-02

## Directory Layout

```
uniclipboard-desktop/
├── src/                                  # Frontend (React 18 + TypeScript + Vite)
│   ├── pages/                           # Route pages (Dashboard, Devices, Settings, Setup)
│   ├── components/                      # Reusable UI components (Shadcn/ui)
│   ├── layouts/                         # Layout wrappers (MainLayout, SettingsFullLayout)
│   ├── store/                           # Redux Toolkit state management
│   ├── api/                             # Tauri command invocations (typed wrappers)
│   ├── contexts/                        # React Context providers (Search, Settings, Shortcuts)
│   ├── hooks/                           # Custom React hooks
│   ├── utils/                           # Frontend utilities
│   ├── lib/                             # Utility functions (cn for class merging)
│   ├── observability/                   # Sentry error tracking setup
│   ├── i18n/                            # Internationalization (English, Chinese, etc.)
│   ├── assets/                          # Static assets (icons, images)
│   ├── styles/                          # Global CSS and theme definitions
│   ├── main.tsx                         # App entry point
│   └── App.tsx                          # Root app component with routing
│
├── src-tauri/                           # Rust backend (Tauri 2 + Tokio)
│   ├── src/                             # Main Tauri entry point
│   │   ├── main.rs                      # Application initialization, command registration
│   │   └── plugins/                     # Platform-specific plugins (macOS rounded corners)
│   │
│   ├── crates/                          # Modular Rust crates (Hexagonal Architecture)
│   │
│   ├── uc-core/                         # Domain layer (zero dependencies)
│   │   └── src/
│   │       ├── clipboard/               # Clipboard domain models and policies
│   │       ├── device/                  # Device aggregate root and identity
│   │       ├── network/                 # Network models and protocol definitions
│   │       ├── security/                # Security domain (encryption state, space access)
│   │       ├── setup/                   # Setup state machines (NewSpace, JoinSpace)
│   │       ├── blob/                    # Blob/binary data domain models
│   │       ├── ids/                     # Strong typed IDs (EntryId, BlobId, etc.)
│   │       ├── ports/                   # Port (interface) definitions
│   │       │   ├── clipboard/           # Clipboard-related ports
│   │       │   ├── security/            # Security-related ports
│   │       │   ├── space/               # Space access/network ports
│   │       │   ├── setup/               # Setup state management ports
│   │       │   └── *.rs                 # Individual port traits
│   │       ├── crypto/                  # Cryptography domain (key derivation, algorithms)
│   │       ├── config/                  # Configuration domain models
│   │       └── app_dirs/                # Application directory abstractions
│   │
│   ├── uc-app/                          # Application/use case layer
│   │   └── src/
│   │       ├── usecases/                # Use case implementations
│   │       │   ├── clipboard/           # Clipboard workflows (list, delete, capture, sync)
│   │       │   ├── encryption/          # Encryption initialization and session management
│   │       │   ├── settings/            # Settings management use cases
│   │       │   ├── pairing/             # Peer pairing orchestration
│   │       │   ├── setup/               # Setup wizard orchestration (NewSpace, JoinSpace)
│   │       │   ├── space_access/        # Space access/network join workflows
│   │       │   └── app_lifecycle/       # App lifecycle coordination
│   │       ├── models/                  # DTOs and response types
│   │       └── App trait                # Core App trait for use case access
│   │
│   ├── uc-infra/                        # Infrastructure/adapter layer (implementations)
│   │   ├── migrations/                  # Diesel database migrations (timestamped)
│   │   └── src/
│   │       ├── db/                      # Database implementations (Diesel)
│   │       │   ├── repositories/        # Repository implementations (ClipboardEntry, Device, etc.)
│   │       │   ├── models/              # Database schema models
│   │       │   ├── mappers/             # Domain ↔ Database model mappers
│   │       │   └── ports/               # Diesel-specific port implementations
│   │       ├── security/                # Security implementations
│   │       │   ├── encryption.rs        # XChaCha20-Poly1305 encryption
│   │       │   ├── hashing/             # Key derivation (Argon2id)
│   │       │   └── secure_storage/      # Stronghold integration
│   │       ├── fs/                      # File system implementations
│   │       │   ├── key_slot_store.rs    # Key material persistence
│   │       │   └── blob_store.rs        # Binary data file storage
│   │       ├── settings/                # Settings TOML persistence
│   │       ├── network/                 # Network utilities and space access adapters
│   │       ├── device/                  # Device repository implementations
│   │       ├── clipboard/               # Clipboard representation handling
│   │       └── time/                    # Timer implementations
│   │
│   ├── uc-platform/                     # Platform adapter layer (OS integration)
│   │   ├── tests/                       # Platform layer tests
│   │   └── src/
│   │       ├── runtime/                 # PlatformRuntime (async event loop)
│   │       │   ├── runtime.rs           # Main event loop and command handling
│   │       │   ├── event_bus.rs         # Channel-based event/command system
│   │       │   └── ipc.rs               # IPC definitions
│   │       ├── clipboard/               # Clipboard platform layer
│   │       │   ├── watcher.rs           # Clipboard watcher callback
│   │       │   ├── platform/            # Platform-specific implementations (macOS, Linux, Windows)
│   │       │   └── local_clipboard.rs   # Adapter for clipboard_rs
│   │       ├── adapters/                # Generic adapters
│   │       │   └── pairing_stream/      # Pairing message framing
│   │       ├── ports/                   # Platform-specific port definitions
│   │       └── ipc/                     # Platform command definitions
│   │
│   ├── uc-tauri/                        # Tauri adapter layer (bridge to frontend)
│   │   ├── tests/                       # Command tests
│   │   ├── src/
│   │   │   ├── bootstrap/               # Dependency wiring and initialization
│   │   │   │   ├── mod.rs               # Main bootstrap entry point
│   │   │   │   ├── runtime.rs           # AppRuntime and UseCases accessor
│   │   │   │   ├── tracing.rs           # Logging initialization
│   │   │   │   └── wire.rs              # Dependency injection setup
│   │   │   ├── commands/                # Tauri command handlers
│   │   │   │   ├── clipboard.rs         # Clipboard commands
│   │   │   │   ├── encryption.rs        # Encryption commands
│   │   │   │   ├── settings.rs          # Settings commands
│   │   │   │   ├── setup.rs             # Setup wizard commands
│   │   │   │   ├── pairing.rs           # Pairing commands
│   │   │   │   ├── lifecycle.rs         # App lifecycle commands
│   │   │   │   └── mod.rs               # Command utilities
│   │   │   ├── models/                  # DTOs (responses/requests)
│   │   │   ├── events/                  # Tauri event definitions and adapters
│   │   │   ├── services/                # High-level business logic wrappers
│   │   │   ├── adapters/                # Port implementations (e.g., lifecycle status)
│   │   │   ├── protocol/                # URI protocol handlers (uc://)
│   │   │   ├── tray/                    # System tray integration
│   │   │   └── shortcut/                # Keyboard shortcut handling
│   │
│   ├── uc-clipboard-probe/              # Utility crate for clipboard inspection
│   │   └── src/                         # CLI tool for debugging clipboard
│   │
│   ├── Cargo.toml                       # Workspace manifest
│   ├── Cargo.lock                       # Dependency lock
│   ├── tauri.conf.json                  # Tauri configuration
│   └── src/main.rs                      # Main Tauri entry
│
├── docs/                                # Architecture and development documentation
│   ├── architecture/                    # Architecture decision records
│   │   ├── commands-status.md           # Tauri command migration status
│   │   └── snapshot-cache/              # Snapshot caching architecture
│   ├── development/                     # Development guides
│   ├── guides/                          # How-to guides
│   └── p2p/                             # Peer-to-peer networking docs
│
├── public/                              # Static assets served by Vite
├── scripts/                             # Build and utility scripts
│   ├── bump-version.js                  # Version bumping script
│   └── __tests__/                       # Script tests
│
├── .github/                             # GitHub Actions workflows
│   └── workflows/                       # CI/CD pipelines
│
├── tsconfig.json                        # TypeScript configuration
├── vite.config.ts                       # Vite build configuration
├── tailwind.config.ts                   # Tailwind CSS configuration
├── eslint.config.js                     # ESLint configuration
├── CLAUDE.md                            # Claude AI project instructions
└── config.toml                          # Development configuration (git-ignored)
```

## Directory Purposes

**Frontend (src/):**

- **pages/**: Route-level components (Dashboard, Devices, Settings, Setup, Unlock)
- **components/**: Reusable UI components organized by feature (clipboard, device, setting)
- **store/**: Redux Toolkit slices and RTK Query API definitions
- **api/**: Type-safe Tauri command wrappers
- **contexts/**: React Context for global state (SearchContext, SettingContext)
- **hooks/**: Custom React hooks (useSearch, usePlatform, useUINavigateListener)

**Backend (src-tauri/):**

- **uc-core/**: Domain models and port interfaces (no external deps)
- **uc-app/**: Use case orchestrators implementing business logic
- **uc-infra/**: Implementations of ports (database, encryption, file system)
- **uc-platform/**: OS-level integration (clipboard watching, app directories)
- **uc-tauri/**: Tauri-specific glue (command handlers, dependency wiring)

**Migrations (src-tauri/crates/uc-infra/migrations/):**

- Format: `YYYY-MM-DD-hhmmss_description`
- Example: `2026-01-09-141527_clipboard_core` (initial clipboard schema)
- Run via Diesel CLI (not automatic)

## Key File Locations

**Entry Points:**

| File                    | Purpose                                                                                |
| ----------------------- | -------------------------------------------------------------------------------------- |
| `src-tauri/src/main.rs` | Backend startup: initialize tracing, load config, wire dependencies, register commands |
| `src/main.tsx`          | Frontend startup: initialize Sentry, Redux store, i18n, render React root              |
| `src/App.tsx`           | App router and authentication state management                                         |

**Configuration:**

| File                        | Purpose                                                        |
| --------------------------- | -------------------------------------------------------------- |
| `src-tauri/tauri.conf.json` | Tauri app metadata, window, dev server port (1420)             |
| `tsconfig.json`             | TypeScript config with `@/*` path alias mapping to `src/*`     |
| `vite.config.ts`            | Frontend build configuration                                   |
| `Cargo.toml` (root)         | Rust workspace manifest                                        |
| `CLAUDE.md`                 | AI assistant project context (logging, architecture, patterns) |

**Core Logic:**

| File                                                                    | Purpose                                    |
| ----------------------------------------------------------------------- | ------------------------------------------ |
| `src-tauri/crates/uc-core/src/ports/clipboard/mod.rs`                   | Clipboard port definitions                 |
| `src-tauri/crates/uc-app/src/usecases/clipboard/mod.rs`                 | Clipboard use cases                        |
| `src-tauri/crates/uc-infra/src/db/repositories/clipboard_entry_repo.rs` | Clipboard database implementation          |
| `src-tauri/crates/uc-tauri/src/commands/clipboard.rs`                   | Clipboard command handlers                 |
| `src/store/api.ts`                                                      | RTK Query clipboard and encryption queries |

**Testing:**

| Location                               | Purpose                              |
| -------------------------------------- | ------------------------------------ |
| `src/**/__tests__/`                    | Frontend component tests             |
| `src-tauri/crates/*/tests/`            | Rust integration tests               |
| `src-tauri/crates/*/src/*.rs` (inline) | Unit tests in `#[cfg(test)]` modules |

## Naming Conventions

**Files:**

- Rust modules: `snake_case.rs` (e.g., `clipboard_entry_repo.rs`, `get_entry_detail.rs`)
- TypeScript files: `camelCase.ts` or `PascalCase.tsx` for components
- Port traits: `*Port.rs` (e.g., `clipboard_entry_repository.rs`)
- Repositories: `*_repo.rs` (e.g., `device_repo.rs`)
- Use cases: `*` or `*_use_case.rs` (e.g., `list_entry_projections.rs`)

**Directories:**

- Feature-based: `clipboard/`, `device/`, `encryption/`, `network/`
- Layer-based: `ports/`, `adapters/`, `repositories/`, `usecases/`
- Platform-based: `platform/` for OS-specific code
- Test directories: `tests/` or `__tests__/`

**Types and Functions:**

- Rust enums: `PascalCase` (e.g., `ClipboardEvent`, `DeviceStatus`)
- Rust structs: `PascalCase` (e.g., `ClipboardEntry`, `LocalClipboard`)
- Rust functions: `snake_case` (e.g., `list_entries`, `create_entry`)
- TypeScript types: `PascalCase` (e.g., `ClipboardEntryProjection`, `EncryptionStatus`)
- React components: `PascalCase` (e.g., `DashboardPage`, `ClipboardList`)
- Redux slices: `camelCase` (e.g., `clipboardSlice`, `devicesSlice`)

## Where to Add New Code

**New Feature Workflow:**

1. **Domain Model** → Add to `uc-core/src/clipboard/` or appropriate domain module
2. **Port Definition** → Add to `uc-core/src/ports/clipboard/` (trait definition)
3. **Use Case** → Create in `uc-app/src/usecases/clipboard/` (orchestration)
4. **Infrastructure** → Add implementation in `uc-infra/src/db/repositories/` or appropriate adapter
5. **Command Handler** → Create in `uc-tauri/src/commands/` that calls use case
6. **Frontend Query** → Add RTK Query hook in `src/store/api.ts`
7. **Frontend Component** → Add page/component in `src/pages/` or `src/components/`
8. **Tests** → Add unit tests next to implementation, integration tests in `tests/`

**New Component/Module:**

- **Frontend Component**: `src/components/[feature]/ComponentName.tsx` with tests in `__tests__/ComponentName.test.tsx`
- **Backend Use Case**: `src-tauri/crates/uc-app/src/usecases/[feature]/description.rs` with ports injected
- **Port Implementation**: `src-tauri/crates/uc-infra/src/[module]/mod.rs` implementing port trait from uc-core

**Utilities:**

- **Frontend helpers**: `src/utils/` or `src/lib/` (prefer `lib/` for UI-related, `utils/` for general)
- **Shared Rust utilities**: Create module in uc-core for domain-related, uc-platform for platform-level
- **API wrappers**: `src/api/` for Tauri command invocations (typed)

## Special Directories

**Generated Directories:**

| Directory           | Generated            | Committed |
| ------------------- | -------------------- | --------- |
| `target/`           | Yes (Rust build)     | No        |
| `dist/`             | Yes (frontend build) | No        |
| `src-tauri/target/` | Yes (Rust workspace) | No        |
| `node_modules/`     | Yes (npm/bun)        | No        |

**Git-Ignored:**

| Path                         | Reason                                 |
| ---------------------------- | -------------------------------------- |
| `.env*`                      | Environment configuration with secrets |
| `config.toml`                | Development configuration              |
| `src-tauri/crates/*/target/` | Build artifacts                        |
| `.DS_Store`                  | macOS metadata                         |
| `*.log`                      | Log files                              |

**Convention Documentation:**

| File                                        | Purpose                                                            |
| ------------------------------------------- | ------------------------------------------------------------------ |
| `docs/architecture/logging-architecture.md` | Logging span and event conventions                                 |
| `CLAUDE.md`                                 | Coding conventions, error handling patterns, architecture overview |
| `docs/architecture/commands-status.md`      | Tauri command migration status to new architecture                 |

## Frontend Structure Details

**Pages (Route-level components):**

- `DashboardPage.tsx` - Main clipboard history and search
- `DevicesPage.tsx` - Paired devices and pairing UI
- `SettingsPage.tsx` - Application settings
- `SetupPage.tsx` - Initial setup wizard (NewSpace or JoinSpace)
- `UnlockPage.tsx` - Encryption unlock (passphrase entry)

**Components:**

- `clipboard/` - ClipboardList, ClipboardEntry, EntryDetail
- `device/` - DeviceCard, DeviceList, PairingForm
- `setting/` - SettingForm, LanguageSelector, SecuritySettings
- `ui/` - Shadcn/ui primitives (Button, Dialog, Input, etc.)
- `layout/` - Layouts with sidebar and main content area

**Store:**

- `appApi.ts` - RTK Query with clipboard, encryption, settings endpoints
- `slices/clipboardSlice.ts` - Pagination, search, selected entry
- `slices/devicesSlice.ts` - Paired devices list
- `slices/statsSlice.ts` - App statistics (entry count, total size)

## Backend Crate Dependency Graph

```
uc-tauri
  ├─→ uc-app         (use cases)
  ├─→ uc-infra       (implementations)
  ├─→ uc-platform    (platform runtime)
  └─→ uc-core        (domain models)

uc-app
  └─→ uc-core        (ports, domain models)

uc-infra
  └─→ uc-core        (ports only, no implementations)

uc-platform
  └─→ uc-core        (ports only, no implementations)

uc-core
  └─→ (nothing)      ← Zero dependencies
```

**Key Rule:** uc-core has zero dependencies on other crates. All implementations provided via dependency injection.

---

_Structure analysis: 2026-03-02_
