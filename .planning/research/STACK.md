# Stack Research

**Domain:** Tauri/Rust desktop architecture remediation (brownfield)
**Researched:** 2026-03-06
**Confidence:** HIGH

## Recommended Stack

### Core Technologies

| Technology | Version                             | Purpose                                      | Why Recommended                                                                                |
| ---------- | ----------------------------------- | -------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Rust       | 1.7x+ toolchain (workspace current) | Core domain/app/infra implementation         | Existing codebase investment is high; remediation should preserve language/runtime continuity. |
| Tokio      | 1.x (workspace uses 1.28/full)      | Async task lifecycle, cancellation, channels | Required to implement structured shutdown and non-blocking driver loops safely.                |
| Tauri      | 2.x                                 | Command boundary and desktop shell           | Existing command surface is where DTO/error contract hardening is needed.                      |
| tracing    | 0.1.x                               | Structured observability                     | Needed to make lifecycle and command failures diagnosable after refactors.                     |

### Supporting Libraries

| Library            | Version                           | Purpose                                         | When to Use                                                                           |
| ------------------ | --------------------------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------- |
| thiserror          | 1.x/2.x compatible with workspace | Typed error enums for ports/app/command mapping | Introduce when replacing `anyhow::Result`/`String` contracts at boundaries.           |
| tokio-util         | 0.7.x                             | `CancellationToken` and task utilities          | Use for lifecycle governance and coordinated shutdown across spawned tasks.           |
| serde              | 1.x                               | Stable DTO serialization                        | Keep domain models internal; expose DTOs only for command/event payload contracts.    |
| mockall (optional) | 0.13.x                            | Targeted trait mocking for app/core tests       | Use selectively when `test_utils` noops are insufficient for behavior-specific tests. |

### Development Tools

| Tool                          | Purpose                                          | Notes                                                                   |
| ----------------------------- | ------------------------------------------------ | ----------------------------------------------------------------------- |
| cargo-nextest (optional)      | Faster and more isolated test execution          | Useful once test matrix expands after decomposition work.               |
| clippy + deny warnings policy | Enforce contract/lifecycle hygiene               | Add checks for `expect`/`unwrap` and layering anti-pattern regressions. |
| cargo-udeps (optional)        | Detect stale dependencies after boundary cleanup | Run after removing horizontal crate dependencies.                       |

## Installation

```bash
# In existing workspace (src-tauri/)
cargo add thiserror --package uc-core
cargo add thiserror --package uc-app
cargo add tokio-util --package uc-platform

# Optional test tooling
cargo install cargo-nextest
cargo install cargo-udeps
```

## Alternatives Considered

| Recommended                     | Alternative                          | When to Use Alternative                                                        |
| ------------------------------- | ------------------------------------ | ------------------------------------------------------------------------------ |
| `thiserror` for typed errors    | Keep `anyhow` everywhere             | Only for internal prototype paths with no API/port boundary exposure.          |
| `tokio-util::CancellationToken` | Ad-hoc bool flags / channel shutdown | Avoid alternatives unless runtime is fully synchronous (not true here).        |
| explicit DTO mapping structs    | Return domain structs directly       | Not acceptable for command API stability; only for private internal functions. |

## What NOT to Use

| Avoid                                                           | Why                                                  | Use Instead                                                    |
| --------------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------------------- |
| More global statics for shared mutable state                    | Amplifies lifecycle bugs and hidden coupling         | Inject state via ports/adapters and runtime-managed ownership. |
| Generic `String` error contracts for commands                   | Breaks frontend handling and traceability            | Typed error enums + explicit DTO error mapping.                |
| Cross-crate horizontal dependencies (`uc-platform -> uc-infra`) | Violates architecture boundary and harms testability | Introduce ports in `uc-core`, implement in adapters only.      |

## Stack Patterns by Variant

**If change is boundary-related:**

- Add/adjust port trait in `uc-core`, then implement in `uc-infra`/`uc-platform`.
- Because boundary correction should be expressed as dependency inversion, not direct crate access.

**If change is command contract-related:**

- Add DTOs and typed command error mapping in command/wiring layer.
- Because command API and domain model should evolve independently.

## Version Compatibility

| Package A        | Compatible With | Notes                                                                       |
| ---------------- | --------------- | --------------------------------------------------------------------------- |
| tauri 2.x        | tokio 1.x       | Current workspace baseline; keep during milestone to reduce migration risk. |
| tracing 0.1.x    | tokio 1.x       | Works with `Instrument` spans required by command tracing rules.            |
| tokio-util 0.7.x | tokio 1.x       | `CancellationToken` integrates directly with existing runtime.              |

## Sources

- Issue scope: https://github.com/UniClipboard/UniClipboard/issues/214
- Workspace manifests: `src-tauri/Cargo.toml` and crate manifests
- Existing project planning context: `.planning/PROJECT.md`, `.planning/MILESTONES.md`

---

_Stack research for: UniClipboard architecture remediation milestone_
_Researched: 2026-03-06_
