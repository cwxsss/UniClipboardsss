# Architecture Research

**Domain:** Hexagonal architecture remediation for Tauri/Rust desktop app
**Researched:** 2026-03-06
**Confidence:** HIGH

## Standard Architecture

### System Overview

```text
┌─────────────────────────────────────────────────────────────┐
│                    Interface / Composition                  │
│   tauri commands, runtime bootstrap, wiring, DTO mapping   │
├─────────────────────────────────────────────────────────────┤
│                      Application Layer                      │
│  use cases / orchestrators using only core ports + models  │
├─────────────────────────────────────────────────────────────┤
│                        Domain Core                          │
│   entities, value objects, ports, domain invariants         │
├─────────────────────────────────────────────────────────────┤
│                     Adapter Implementations                 │
│  infra/platform adapters implementing core ports            │
└─────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component                  | Responsibility                      | Typical Implementation                               |
| -------------------------- | ----------------------------------- | ---------------------------------------------------- |
| `uc-core` ports/models     | Define contracts and invariants     | Traits + domain types, no external impl dependencies |
| `uc-app` use cases         | Coordinate business operations      | Orchestrators/services using injected ports          |
| `uc-infra` adapters        | Data/crypto/storage implementations | Concrete adapters behind port traits                 |
| `uc-platform` adapters     | OS/runtime/network integrations     | Platform-specific adapters behind port traits        |
| `uc-tauri` commands/wiring | DTO mapping + app composition       | command handlers + runtime builder                   |

## Recommended Project Structure

```text
src-tauri/crates/
├── uc-core/
│   ├── src/models/
│   └── src/ports/
├── uc-app/
│   ├── src/usecases/
│   ├── src/services/
│   └── src/deps/
├── uc-infra/
│   └── src/{db,security,storage,...}/
├── uc-platform/
│   └── src/adapters/{clipboard,network,runtime,...}/
└── uc-tauri/
    └── src/{commands,bootstrap,dto,error}/
```

### Structure Rationale

- **Ports in core, impls in adapters:** forces dependency inversion and prevents horizontal adapter coupling.
- **DTO/error contracts at tauri boundary:** prevents frontend coupling to domain model churn.
- **Use-case decomposition:** isolates business steps and reduces orchestrator bloat.

## Architectural Patterns

### Pattern 1: Port-First Boundary Repair

**What:** Add/adjust port in `uc-core`, then inject adapter implementation from infra/platform.
**When to use:** Any direct cross-layer crate access or leaked concrete implementation.
**Trade-offs:** More trait/wiring code, but strong decoupling and testability.

### Pattern 2: DTO Translation at Command Boundary

**What:** Commands return DTOs/errors, use cases stay domain-focused.
**When to use:** All tauri command outputs/events.
**Trade-offs:** Mapping maintenance cost, but API stability and explicit contracts.

### Pattern 3: Task Governance via Managed Runtime

**What:** Central task manager owns join handles + cancellation tokens.
**When to use:** Long-lived async loops or network/session workers.
**Trade-offs:** More lifecycle plumbing, but deterministic shutdown and fewer leaked tasks.

## Data Flow

### Request Flow

```text
[Frontend Command]
    -> [tauri command handler]
    -> [use case]
    -> [core port]
    -> [infra/platform adapter]
    <- [domain result]
    <- [DTO mapper + typed error mapper]
```

### State Management

```text
[Runtime State Owner]
    -> creates [TaskManager + shared services]
    -> injects into [UseCases]
    -> exposes controlled handles to [Commands]
```

### Key Data Flows

1. **Sync operation flow:** command -> use case -> network/storage ports -> adapters; no command-level deps bypass.
2. **Lifecycle shutdown flow:** app close -> task manager cancel -> adapters flush/close -> runtime drop complete.

## Scaling Considerations

| Scale                   | Architecture Adjustments                                                                |
| ----------------------- | --------------------------------------------------------------------------------------- |
| Current desktop scope   | Monolith workspace is fine; prioritize boundary integrity over service split.           |
| Growing feature surface | Split use-case modules/services by bounded context; keep shared runtime contracts thin. |
| High contributor count  | Enforce crate dependency checks + architectural linting gates in CI.                    |

### Scaling Priorities

1. **First bottleneck:** orchestration complexity in large use cases -> split into composable services.
2. **Second bottleneck:** contract drift between frontend and domain -> enforce DTO and traceability tests.

## Anti-Patterns

### Anti-Pattern 1: Convenience Penetration (`runtime.deps` direct use)

**What people do:** Read/write infra deps from command handlers.
**Why it's wrong:** Breaks layering and bypasses business invariants.
**Do this instead:** Add/extend use cases and keep runtime internals private.

### Anti-Pattern 2: Adapter-to-Adapter Dependency

**What people do:** Make `uc-platform` depend on `uc-infra` helper structs.
**Why it's wrong:** Horizontal dependency creates fragile cycles and testing pain.
**Do this instead:** Move contract to core port and inject implementation.

## Integration Points

### External Services

| Service              | Integration Pattern                 | Notes                                                |
| -------------------- | ----------------------------------- | ---------------------------------------------------- |
| Clipboard OS API     | `ClipboardPort` adapter             | Keep OS peculiarities in `uc-platform` only.         |
| libp2p networking    | network ports + adapter events      | Avoid leaking libp2p specifics into use cases.       |
| Local storage/crypto | storage/security ports + `uc-infra` | Typed errors should preserve recoverable categories. |

### Internal Boundaries

| Boundary              | Communication              | Notes                                                 |
| --------------------- | -------------------------- | ----------------------------------------------------- |
| `uc-tauri` ↔ `uc-app` | Use-case API only          | DTO mapping lives in `uc-tauri`, not `uc-app`.        |
| `uc-app` ↔ `uc-core`  | Domain models + ports      | `uc-app` consumes abstractions, no concrete adapters. |
| `uc-app` ↔ adapters   | Through port trait objects | Composition root wires implementations.               |

## Sources

- Issue #214 architecture review and phased remediation plan
- Existing planning docs (`PROJECT.md`, `MILESTONES.md`, codebase architecture notes)

---

_Architecture research for: UniClipboard architecture remediation_
_Researched: 2026-03-06_
