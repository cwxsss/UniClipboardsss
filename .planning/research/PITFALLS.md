# Pitfalls Research

**Domain:** In-place architecture remediation of production desktop app
**Researched:** 2026-03-06
**Confidence:** HIGH

## Critical Pitfalls

### Pitfall 1: Big-Bang Refactor Across All Clusters

**What goes wrong:** Multiple clusters (A-E) changed at once, causing regressions that are hard to isolate.

**Why it happens:** Architectural debt feels interconnected, so teams try one massive correction.

**How to avoid:** Use phased requirement mapping with atomic intent per phase and contract tests between phases.

**Warning signs:** PRs span boundary fixes + lifecycle + DTO + cleanup in one diff.

**Phase to address:** Early phase planning and roadmap gating.

---

### Pitfall 2: Boundary Fixes That Keep Hidden Shortcuts

**What goes wrong:** Visible dependency removed, but commands still bypass use cases via private shortcuts.

**Why it happens:** Convenience pressure during migration.

**How to avoid:** Add explicit command/usecase coverage checks and grep-based guards for forbidden access patterns.

**Warning signs:** New helper methods expose `deps`-like internals, or command files still import adapter internals.

**Phase to address:** Boundary remediation phase.

---

### Pitfall 3: Typed Errors Introduced Inconsistently

**What goes wrong:** Some paths return structured errors while others still collapse to `String`/`anyhow`.

**Why it happens:** Partial migration without contract boundary policy.

**How to avoid:** Define command error taxonomy first, then migrate command endpoints in a tracked batch.

**Warning signs:** Frontend still pattern-matches on ad-hoc strings; mixed error shapes in tests.

**Phase to address:** DTO/error contract phase.

---

### Pitfall 4: Cancellation Added Without Ownership Model

**What goes wrong:** Tokens exist but not propagated, tasks outlive runtime, and shutdown remains flaky.

**Why it happens:** Cancellation treated as utility, not lifecycle architecture.

**How to avoid:** Introduce task manager that owns token roots + join handles and validates shutdown completion.

**Warning signs:** Detached spawned tasks, missing join waits, shutdown hangs/timeouts.

**Phase to address:** Lifecycle governance phase.

---

### Pitfall 5: Decomposition Without Behavioral Safety Nets

**What goes wrong:** God objects split, but invariant behavior changes silently.

**Why it happens:** Refactor-first execution without scenario verification.

**How to avoid:** Add scenario-level regression checks for pairing, sync, and setup flows before decomposition.

**Warning signs:** “Pure refactor” claims with no behavior assertions.

**Phase to address:** Responsibility decomposition phase.

## Technical Debt Patterns

| Shortcut                            | Immediate Benefit        | Long-term Cost                        | When Acceptable                                        |
| ----------------------------------- | ------------------------ | ------------------------------------- | ------------------------------------------------------ |
| Keep global mutable state           | Easy cross-module access | Hidden coupling, shutdown bugs        | Never for runtime-critical state                       |
| Keep domain structs as command DTOs | Less mapping boilerplate | Frontend breakage on domain evolution | Only for temporary internal debug commands             |
| Temporary `pub` access for deps     | Fast migration           | Permanent architecture drift          | Only within short-lived branch with immediate reversal |

## Integration Gotchas

| Integration                | Common Mistake                                        | Correct Approach                                                      |
| -------------------------- | ----------------------------------------------------- | --------------------------------------------------------------------- |
| Tauri command -> use case  | Returning domain model directly                       | Map to explicit DTO with camelCase serialization where needed         |
| Network adapter -> decoder | Direct concrete dependency to another adapter crate   | Depend on core port abstraction and inject implementation             |
| Runtime shutdown           | Dropping runtime without coordinated task cancel/join | Centralized task manager + cancellation propagation + join monitoring |

## Performance Traps

| Trap                                             | Symptoms                            | Prevention                                               | When It Breaks             |
| ------------------------------------------------ | ----------------------------------- | -------------------------------------------------------- | -------------------------- |
| Over-synchronization during lifecycle hardening  | Increased latency in sync path      | Keep locks localized, avoid blocking in poll loops       | Under concurrent peer sync |
| Excessive DTO cloning for large payload metadata | Higher memory churn in command path | Keep DTOs lightweight and avoid payload duplication      | Large image sync bursts    |
| Test suite inflation with heavy deps setup       | Slow CI feedback loops              | Shared test builders/noops and per-module targeted tests | As use-case count grows    |

## Security Mistakes

| Mistake                                          | Risk                                  | Prevention                                     |
| ------------------------------------------------ | ------------------------------------- | ---------------------------------------------- |
| Logging sensitive clipboard payload contents     | Data exposure in logs                 | Log sizes/hashes only, redact content always   |
| Error translation dropping security context      | Misdiagnosed crypto/session failures  | Preserve category-safe context in typed errors |
| Bypassing encryption session use case invariants | Inconsistent encrypted transfer state | Enforce command access via use cases only      |

## UX Pitfalls

| Pitfall                               | User Impact                        | Better Approach                                               |
| ------------------------------------- | ---------------------------------- | ------------------------------------------------------------- |
| Refactor regresses setup state events | Setup UI appears stuck             | Contract tests for emitted payload shapes and required fields |
| Shutdown hangs during app close       | User perceives crashes/freezes     | Deterministic cancellation + bounded shutdown timeout         |
| Silent command error shape drift      | UI cannot show actionable feedback | Versioned/typed command error schema                          |

## "Looks Done But Isn't" Checklist

- [ ] **Boundary repair:** no remaining command-level bypasses or horizontal adapter deps.
- [ ] **Error contracts:** all migrated commands use typed error mapping, no fallback `String` path.
- [ ] **Lifecycle governance:** close/restart scenarios verify all long-lived tasks terminate.
- [ ] **Decomposition:** key user flows pass regression checks post-split.

## Recovery Strategies

| Pitfall                                      | Recovery Cost | Recovery Steps                                                                           |
| -------------------------------------------- | ------------- | ---------------------------------------------------------------------------------------- |
| Big-bang change merged                       | HIGH          | Re-split into phased commits, restore stable baseline, reapply incrementally             |
| Partial typed-error migration                | MEDIUM        | Freeze API, add adapter layer for old/new mapping, complete migration sweep              |
| Shutdown instability after lifecycle changes | HIGH          | Add instrumentation, identify non-terminating tasks, enforce cancellation/join contracts |

## Pitfall-to-Phase Mapping

| Pitfall                      | Prevention Phase                   | Verification                                             |
| ---------------------------- | ---------------------------------- | -------------------------------------------------------- |
| Big-bang refactor            | Roadmap definition + phase slicing | Each phase maps isolated requirements with atomic intent |
| Hidden boundary shortcuts    | Boundary remediation               | No forbidden imports/access patterns in command layer    |
| Error contract inconsistency | DTO/error contract phase           | Command tests assert stable typed error schema           |
| Token-only cancellation      | Lifecycle governance               | Close/restart tests confirm all tasks terminate          |
| Unsafe decomposition         | Decomposition phase                | Sync/setup/pairing regression suite remains green        |

## Sources

- Issue #214 merged findings and phase plan
- Existing milestone and architecture planning context

---

_Pitfalls research for: UniClipboard architecture remediation_
_Researched: 2026-03-06_
