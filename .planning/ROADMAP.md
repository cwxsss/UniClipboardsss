# Roadmap: UniClipboard Desktop

## Milestones

- ✅ **v0.1.0 Daily Driver** - Phases 1-9 (shipped 2026-03-06)
- 🚧 **v0.1.1 Architecture Remediation** - Phases 10-13 (in progress)

## Phases

<details>
<summary>✅ v0.1.0 Daily Driver (Phases 1-9) - SHIPPED 2026-03-06</summary>

See: `.planning/milestones/v0.1.0-ROADMAP.md`

</details>

### 🚧 v0.1.1 Architecture Remediation (In Progress)

**Milestone Goal:** Eliminate root-cause architecture defects from issue #214 while preserving daily-driver clipboard sync behavior.

#### Phase 10: Boundary Repair Baseline

**Goal**: Restore strict dependency direction and close command-layer penetration paths.
**Depends on**: Phase 9 (milestone baseline)
**Requirements**: BOUND-01, BOUND-02, BOUND-03, BOUND-04
**Success Criteria** (what must be TRUE):

1. Commands perform business operations through use cases rather than direct runtime deps access.
2. Runtime dependency containers are no longer externally exposed to command modules.
3. Platform decode path uses core-defined port contracts instead of cross-adapter crate coupling.
4. Non-domain ports are removed from core and referenced through proper outer-layer placement.
   **Plans**: 3 plans

Plans:

- [ ] 10-01: Lock down runtime/usecase access boundaries and remove command bypasses
- [ ] 10-02: Introduce/route transfer decode abstraction through core port contracts
- [ ] 10-03: Move non-domain ports out of core and update wiring/tests

#### Phase 11: Command Contract Hardening

**Goal**: Establish stable DTO and typed error contracts for command surfaces.
**Depends on**: Phase 10
**Requirements**: CONTRACT-01, CONTRACT-02, CONTRACT-03, CONTRACT-04
**Success Criteria** (what must be TRUE):

1. Command endpoints return DTOs instead of leaking domain models.
2. Command failures use structured typed error categories, not generic `String` responses.
3. Event/command payload serialization is frontend-compatible and verified by tests.
4. Timeout, cancellation, and internal failures are distinguishable at command boundary.
   **Plans**: 2 plans

Plans:

- [ ] 11-01: Implement command DTO mapping layer and payload contract tests
- [ ] 11-02: Introduce typed command error taxonomy and migrate command handlers

#### Phase 12: Lifecycle Governance Baseline

**Goal**: Make async task lifecycle deterministic through cancellation and graceful shutdown governance.
**Depends on**: Phase 11
**Requirements**: LIFE-01, LIFE-02, LIFE-03, LIFE-04
**Success Criteria** (what must be TRUE):

1. App close/restart does not leave orphaned sync/pairing tasks.
2. Spawned workers are tracked and shutdown with bounded cancellation + join behavior.
3. Staging/session state is lifecycle-owned and no longer managed by unsafe globals.
4. Encryption/session behavior has one authoritative implementation path.
   **Plans**: 2 plans

Plans:

- [ ] 12-01: Introduce task ownership/cancellation framework and runtime shutdown flow
- [ ] 12-02: Remove unmanaged global state and unify encryption/session ownership path

#### Phase 13: Responsibility Decomposition & Testability

**Goal**: Reduce god-object complexity and improve maintainability/test velocity with guarded refactors.
**Depends on**: Phase 12
**Requirements**: DECOMP-01, DECOMP-02, DECOMP-03, DECOMP-04
**Success Criteria** (what must be TRUE):

1. High-risk sync/setup use-case modules are split into focused components with clear responsibilities.
2. Dependency organization is grouped to reduce broad god-container coupling.
3. Shared test helpers/noops reduce duplicate scaffold code and setup overhead.
4. Regression checks confirm pairing, sync, and setup flows remain stable post-decomposition.
   **Plans**: 3 plans

Plans:

- [ ] 13-01: Decompose high-risk sync/setup use cases into focused services/components
- [ ] 13-02: Reorganize app dependency containers for maintainable bounded contexts
- [ ] 13-03: Consolidate shared test helpers and add flow-level regression coverage

## Progress

| Phase                                          | Milestone | Plans Complete | Status      | Completed |
| ---------------------------------------------- | --------- | -------------- | ----------- | --------- |
| 10. Boundary Repair Baseline                   | v0.1.1    | 0/3            | Not started | -         |
| 11. Command Contract Hardening                 | v0.1.1    | 0/2            | Not started | -         |
| 12. Lifecycle Governance Baseline              | v0.1.1    | 0/2            | Not started | -         |
| 13. Responsibility Decomposition & Testability | v0.1.1    | 0/3            | Not started | -         |
