# UniClipboard Documentation

UniClipboard Desktop is a cross-platform clipboard synchronization tool built with Tauri 2, React, and Rust, following **Hexagonal Architecture (Ports and Adapters)**.

## Quick Navigation

**For New Developers:**

- [Project Overview](overview.md) - What is UniClipboard and how it works
- [Architecture Principles](architecture/principles.md) - Understanding Hexagonal Architecture

**For Implementation:**

- [Bootstrap System](architecture/bootstrap.md) - How dependency injection works
- [Module Boundaries](architecture/module-boundaries.md) - What each module can/cannot do
- [Snapshot Cache Pipeline ADR](architecture/snapshot-cache/adr-001-snapshot-cache-pipeline.md) - Cache/spool/worker design decisions
- [Error Handling](guides/error-handling.md) - Error handling strategy
- [GitHub Releases Updater](guides/github-releases-updater.md) - Auto-update pipeline with latest.json

**For Code Review:**

- [Coding Standards](standards/coding-standards.md) - Code style and conventions
- [Module Boundaries](architecture/module-boundaries.md) - Architecture compliance checklist

**For Reference:**

- [DeepWiki Documentation](https://deepwiki.com/UniClipboard/UniClipboard) - Interactive diagrams and flows
- [Archive](archive/) - Historical planning documents

## Architecture at a Glance

```
┌─────────────────────────────────────────────────────────────┐
│                         Tauri App                           │
│  ┌──────────────────────────────────────────────────────┐  │
│  │                   uc-tauri                            │  │
│  │  ┌────────────────────────────────────────────────┐  │  │
│  │  │              bootstrap (Wiring)                │  │  │
│  │  └────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                      uc-app                                 │
│  (Use Cases, Application Logic, Port-Only Dependencies)      │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                      uc-core                                │
│           (Domain Models, Port Definitions)                  │
└─────────────────────────────────────────────────────────────┘
                            ↑
        ┌───────────────────┴───────────────────┐
        │                                       │
┌──────────────────┐                  ┌──────────────────┐
│   uc-infra       │                  │  uc-platform     │
│ (Database, File  │                  │ (Clipboard,      │
│  System, Crypto) │                  │  Network, OS)    │
└──────────────────┘                  └──────────────────┘
```

**Key Principle**: `uc-app` depends only on `uc-core` (Ports). Implementations in `uc-infra` and `uc-platform` are injected by `uc-tauri::bootstrap`.

## Crate Structure

```
src-tauri/crates/
├── uc-core/         # Pure domain layer (Port definitions)
├── uc-infra/        # Infrastructure implementations (DB, FS, crypto)
├── uc-platform/     # Platform adapters (Clipboard, Network, OS)
├── uc-app/          # Application layer (Use cases, business logic)
└── uc-tauri/        # Tauri integration (Commands, Bootstrap)
```

## Current Migration Status

The project is transitioning from Clean Architecture to Hexagonal Architecture (~60% complete).

**Completed**:

- ✅ Core domain layer (uc-core) with port definitions
- ✅ Infrastructure layer (uc-infra) with repository implementations
- ✅ Platform layer (uc-platform) with OS adapters
- ✅ Bootstrap module for dependency injection
- ✅ Application layer (uc-app) with use case structure

**In Progress**:

- 🔄 Use case implementations (remaining work in new crates)
- 🔄 Tauri command migration to new architecture

**Legacy Code**:

- Legacy `src-tauri/src-legacy/` directory has been removed (2026-02-26)

## Getting Started

1. **Read** [Project Overview](overview.md) to understand the system
2. **Study** [Architecture Principles](architecture/principles.md) to grasp the design
3. **Review** [Module Boundaries](architecture/module-boundaries.md) before making changes
4. **Follow** [Coding Standards](standards/coding-standards.md) when implementing

## Development Workflow

```bash
# Install dependencies (uses Bun)
bun install

# Start development server
bun tauri dev

# Run tests
cargo test --workspace

# Build for production
bun tauri build
```

## Documentation Guide

### How to Use These Documents

**When implementing a feature:**

1. Check [Module Boundaries](architecture/module-boundaries.md) to understand which crates are involved
2. Review [Bootstrap System](architecture/bootstrap.md) to see how to inject dependencies
3. Follow [Error Handling](guides/error-handling.md) for proper error propagation

**When reviewing code:**

1. Verify architecture compliance using [Module Boundaries](architecture/module-boundaries.md) checklists
2. Check [Coding Standards](standards/coding-standards.md) for style and conventions
3. Ensure error handling follows [Error Handling](guides/error-handling.md) strategy

**When making architectural decisions:**

1. Reference [Architecture Principles](architecture/principles.md) for core principles
2. Review [Bootstrap System](architecture/bootstrap.md) for dependency injection patterns
3. Consult archived plans in [archive/](archive/) for historical context

### Document Conventions

- **✅ Allowed**: What you SHOULD do
- **❌ Prohibited**: What you MUST NOT do
- **⚠️ Warning**: Common pitfalls to avoid
- **Iron Rule**: Critical architectural constraint that cannot be violated

## Contributing to Documentation

When updating documentation:

1. Keep it focused on **principles**, not implementation details
2. Use **examples** from actual code when possible
3. Update **cross-references** if moving or renaming sections
4. **Avoid duplication** - link to existing sections instead of repeating

## Additional Resources

- **Project DeepWiki**: https://deepwiki.com/UniClipboard/UniClipboard (interactive diagrams)
- **GitHub Repository**: https://github.com/UniClipboard/UniClipboard
- **CLAUDE.md**: Project-specific instructions for Claude Code (in repository root)
