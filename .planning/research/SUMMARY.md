# Project Research Summary

**Project:** UniClipboard Desktop
**Domain:** Brownfield architecture remediation for encrypted clipboard sync desktop app
**Researched:** 2026-03-06
**Confidence:** HIGH

## Executive Summary

This milestone is a brownfield remediation effort, not a net-new feature build. Research indicates the fastest safe path is stack continuity (Tauri 2 + Tokio + tracing) with targeted additions only where contracts/lifecycle require them (`thiserror`, `tokio-util`). The core delivery risk is regression from mixed-intent refactors; phase isolation and requirement traceability are mandatory.

The required capability set is clear: boundary repair, command DTO/error hardening, lifecycle governance, selective decomposition of god modules, and test infrastructure reduction. These are table stakes for architectural reliability in this codebase, and they directly map to issue #214 clusters A-E.

Main risk areas are hidden shortcuts (command bypasses), partial typed-error adoption, and cancellation without ownership model. Each must have explicit prevention criteria in roadmap phases.

## Key Findings

### Recommended Stack

Keep current workspace stack and avoid framework churn. Add minimal support libraries only where required by milestone intent.

**Core technologies:**

- Rust: preserve existing architecture/workspace investment
- Tokio 1.x: lifecycle/task orchestration baseline
- Tauri 2.x: command boundary where contract hardening happens
- tracing 0.1.x: observability for async/lifecycle correctness

### Expected Features

**Must have (table stakes):**

- Boundary integrity restoration without user-visible sync regressions
- Stable command DTO + typed error contract surfaces
- Unified lifecycle cancellation/shutdown behavior

**Should have (competitive):**

- Faster test iteration via shared test helpers and lower setup overhead
- Reduced maintenance risk by decomposing highest-risk god modules

**Defer (v2+):**

- Broad non-essential framework/runtime migrations
- Deep refactors unrelated to issue #214 clusters

### Architecture Approach

Use a strict port-first remediation sequence: define/adjust core ports, implement adapters behind those ports, enforce command->usecase-only access, and keep DTO/error mapping at tauri boundary. Pair this with task-manager-based lifecycle ownership and phased decomposition to avoid broad regressions.

**Major components:**

1. Boundary repair layer (ports + runtime visibility controls)
2. Command contract layer (DTO mapping + typed error taxonomy)
3. Lifecycle governance layer (task manager + cancellation propagation)
4. Decomposition/testability layer (split high-risk orchestrators, shared test scaffolding)

### Critical Pitfalls

1. **Big-bang remediation** — avoid by phase-isolated requirements and atomic intent.
2. **Hidden bypasses after boundary fixes** — avoid with strict command import/access checks.
3. **Partial typed-error migration** — avoid with upfront taxonomy and migration sweep.
4. **Cancellation without ownership** — avoid with centralized task manager and join tracking.
5. **Decomposition without regression guards** — avoid with flow-level verification before/after splits.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 10: Boundary Repair Baseline

**Rationale:** Boundary leaks drive several downstream symptoms.
**Delivers:** Port-based replacement for known horizontal dependencies and command bypass closures.
**Addresses:** Cluster A core requirements.
**Avoids:** Hidden coupling and adapter cross-dependencies.

### Phase 11: Command Contract Hardening

**Rationale:** API stability should be fixed before broader decomposition.
**Delivers:** DTO mapping and typed command error baseline.
**Uses:** `thiserror`/contract mapping conventions.
**Implements:** Cluster C baseline requirements.

### Phase 12: Lifecycle Governance Baseline

**Rationale:** Runtime safety is high user-impact risk and must be deterministic.
**Delivers:** Cancellation propagation, task ownership, graceful shutdown behavior.
**Implements:** Cluster D baseline requirements.

### Phase 13: Responsibility Decomposition + Testability

**Rationale:** Safe after boundaries/contracts/lifecycle are stabilized.
**Delivers:** Initial split of god modules and shared test utility improvements.
**Implements:** Cluster B and cluster E initial scope.

### Phase Ordering Rationale

- Boundary and API contracts first reduce change coupling for later structural work.
- Lifecycle before deep decomposition prevents hidden shutdown regressions.
- Testability improvements after boundary cleanup prevent reinforcing old anti-patterns.

### Research Flags

Phases likely needing deeper research during planning:

- **Phase 10:** port extraction details around network decode paths and crate boundaries.
- **Phase 12:** shutdown semantics for all long-lived async loops.

Phases with standard patterns (skip deep research):

- **Phase 11:** DTO and typed-error migration patterns are established.

## Confidence Assessment

| Area         | Confidence | Notes                                                               |
| ------------ | ---------- | ------------------------------------------------------------------- |
| Stack        | HIGH       | Existing stack already aligns; changes are incremental.             |
| Features     | HIGH       | Scope is explicit in issue #214 clusters and current planning docs. |
| Architecture | HIGH       | Target layering is already defined; remediation path is concrete.   |
| Pitfalls     | HIGH       | Risks are directly observed from current codebase review findings.  |

**Overall confidence:** HIGH

### Gaps to Address

- Exact requirement granularity per cluster still needs user-confirmed scoping decisions.
- Phase-by-phase regression suite definition should be finalized during roadmap creation.

## Sources

### Primary (HIGH confidence)

- https://github.com/UniClipboard/UniClipboard/issues/214 — root-cause clusters and phased remediation plan
- `.planning/PROJECT.md`, `.planning/MILESTONES.md` — current product/milestone context
- `src-tauri/Cargo.toml` — current stack baseline

### Secondary (MEDIUM confidence)

- `.planning/codebase/ARCHITECTURE.md` — architecture context map

---

_Research completed: 2026-03-06_
_Ready for roadmap: yes_
