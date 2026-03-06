# Feature Research

**Domain:** Architecture remediation for an existing encrypted clipboard-sync desktop app
**Researched:** 2026-03-06
**Confidence:** HIGH

## Feature Landscape

### Table Stakes (Users Expect These)

| Feature                                                       | Why Expected                                                  | Complexity | Notes                                                                        |
| ------------------------------------------------------------- | ------------------------------------------------------------- | ---------- | ---------------------------------------------------------------------------- |
| No behavior regressions in sync/pairing                       | Refactor milestone must not break daily-driver workflows      | HIGH       | Every remediation phase needs user-visible invariants and regression checks. |
| Strict boundary conformance (`uc-app -> uc-core <- adapters`) | Team expects hexagonal layering promises to hold              | MEDIUM     | Must remove known horizontal dependencies and penetration paths.             |
| Graceful shutdown/cancellation for spawned tasks              | Users expect app stability during close/restart/network churn | HIGH       | Needs unified task ownership and runtime shutdown policy.                    |
| Stable command contracts (DTO + typed errors)                 | Frontend requires predictable payload/error shapes            | MEDIUM     | Required to prevent silent mismatch and brittle UI handling.                 |

### Differentiators (Competitive Advantage)

| Feature                                                        | Value Proposition                                | Complexity | Notes                                                      |
| -------------------------------------------------------------- | ------------------------------------------------ | ---------- | ---------------------------------------------------------- |
| Roadmap-driven cluster remediation (A-E) with traceability     | High confidence delivery for deep technical debt | MEDIUM     | Lets team ship architectural trust, not just ad-hoc fixes. |
| Testability acceleration (`with_all_noop` + shared test utils) | Faster iteration on future feature work          | MEDIUM     | Reduces per-usecase test setup burden significantly.       |
| Progressive decomposition of god objects                       | Lowers long-term change risk and onboarding cost | HIGH       | Must be phased to avoid destabilizing runtime wiring.      |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature                                            | Why Requested                    | Why Problematic                                        | Alternative                                                                          |
| -------------------------------------------------- | -------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| Big-bang “rewrite architecture first”              | Feels clean and decisive         | High regression risk; blocks delivery and verification | Phased remediation with explicit requirement-to-phase mapping.                       |
| Introduce new runtime/framework during remediation | Looks like a chance to modernize | Mixes migration risk with defect-removal risk          | Keep stack stable; only add minimal libraries (`thiserror`, `tokio-util`) if needed. |
| Expose more internals for convenience (`pub deps`) | Faster command-side coding       | Reintroduces layer penetration immediately             | Add missing use cases/ports and keep deps private.                                   |

## Feature Dependencies

```text
Boundary repair
    └──requires──> port-first abstractions + private runtime deps
                       └──requires──> command/usecase access-path cleanup

Typed command contracts
    └──requires──> DTO mapping conventions + typed error enums

Lifecycle governance
    └──requires──> task ownership model + cancellation token propagation

Testability foundation
    └──requires──> stabilized boundaries + dependency grouping cleanup
```

### Dependency Notes

- **Command contract hardening requires boundary cleanup:** typed contracts degrade quickly if command layer can bypass use cases.
- **Lifecycle governance depends on ownership clarity:** cancellation cannot be reliable with hidden global state.
- **Testability improvements compound after decomposition:** reducing god containers makes no-op/default test wiring tractable.

## MVP Definition

### Launch With (v0.2.0)

- [ ] Boundary repairs for top violations from issue #214 (cluster A baseline)
- [ ] Typed command DTO/error contract baseline (cluster C baseline)
- [ ] Lifecycle/task shutdown governance baseline (cluster D baseline)
- [ ] Initial decomposition of highest-risk god modules (cluster B initial slice)

### Add After Validation (v0.1.x)

- [ ] Broader port typing migration away from `anyhow` across all ports
- [ ] Wider domain-model refinement where anemic models still leak complexity

### Future Consideration (v0.2+)

- [ ] Larger structural redesigns that are not needed for current defect clusters
- [ ] Runtime/framework upgrades unrelated to remediation outcomes

## Feature Prioritization Matrix

| Feature                         | User Value | Implementation Cost | Priority |
| ------------------------------- | ---------- | ------------------- | -------- |
| Boundary violation removal      | HIGH       | MEDIUM              | P1       |
| Lifecycle cancellation/shutdown | HIGH       | HIGH                | P1       |
| Command DTO + typed errors      | HIGH       | MEDIUM              | P1       |
| God object decomposition        | MEDIUM     | HIGH                | P2       |
| Test harness consolidation      | MEDIUM     | MEDIUM              | P2       |

**Priority key:**

- P1: Must have for milestone integrity
- P2: Should complete in milestone if decomposition remains safe
- P3: Future consideration

## Competitor Feature Analysis

| Feature                    | Competitor A  | Competitor B  | Our Approach                                                                  |
| -------------------------- | ------------- | ------------- | ----------------------------------------------------------------------------- |
| Architecture observability | Internal only | Internal only | Treat as engineering product feature with explicit requirements/traceability. |
| Error contract discipline  | Mixed         | Mixed         | Mandate structured command error mapping and DTO boundaries.                  |
| Lifecycle governance       | Varies        | Varies        | Explicit cancellation and shutdown criteria per phase.                        |

## Sources

- Architecture review issue: #214
- Existing project planning docs and archived milestone outputs

---

_Feature research for: UniClipboard architecture remediation_
_Researched: 2026-03-06_
