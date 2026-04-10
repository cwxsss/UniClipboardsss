# Architecture and Commit Rules

Use this document when changes touch module boundaries, cross-crate types, commit structure, or review scope.

## Hexagonal Architecture Boundaries

- **Layering is fixed:**
  - `uc-app → uc-core ← uc-infra / uc-platform`
- **Core isolation is non-negotiable:**
  - `uc-core` must **not** depend on any external implementations.
- **All external capabilities go through Ports (no exceptions):**
  - DB, FS, Clipboard, Network, Crypto

## Cross-Crate Type Conversion Rules

### 1. Never add orphan-rule-violating conversions

If both the source type and target type are defined in other crates, do **not** write:

```rust
impl From<ExternalA> for ExternalB
```

Required handling:

- If the conversion belongs to a transport/API layer, keep it in that layer via a local projection module.
- Prefer a local trait or a local mapper module owned by the current crate.
- Do not push transport mapping into `uc-core` just to recover `From`/`Into` ergonomics.

### 2. Do not spread mechanical `*_from` / `*_to` helpers across the codebase

One or two local helpers are acceptable during a narrow refactor. They must not become the default cross-crate conversion pattern.

Required handling:

- Create a dedicated projection/mapping module near the owning boundary.
- For pure self-to-target projections, prefer a local trait such as `IntoApiDto<T>`.
- For context-dependent projection, use a clearly owned mapper function in that same module.

### 3. Stable enum string rules must have a single authority

If an enum's string representation is used in more than one crate, it is a shared rule — not a local helper.

Required handling:

```rust
impl std::fmt::Display for MyEnum { ... }
impl std::str::FromStr for MyEnum { ... }
```

Then call:

```rust
value.to_string()
MyEnum::from_str(raw)
```

Do not keep duplicated helpers like these in multiple crates:

```rust
fn my_enum_to_string(...)
fn my_enum_to_str(...)
fn my_enum_from_str(...)
```

Those helpers are acceptable only as a short-lived local step during a refactor. Once the enum is known to have a stable cross-crate string form, move that rule to the enum's owning crate and delete the local helpers.

### 4. Decide ownership before choosing the conversion mechanism

Before writing any conversion, answer:

- Which crate owns the source type semantics?
- Which crate owns the target type semantics?
- Is this conversion domain-level, persistence-level, or transport-level?
- Where should the single source of truth live?

### 5. Anti-pattern checklist for cross-crate projection work

Stop and restructure if you see any of these:

- the same enum-to-string mapping repeated in `uc-core`, `uc-app`, `uc-infra`, or `uc-daemon`
- a review suggestion that replaces an invalid `From` impl with many mechanical `*_from` or `*_to` helpers spread across call sites
- transport projection logic leaking into `uc-core` only to recover `.into()` ergonomics
- old helper paths kept after a projection layer or enum authority has been introduced

Preferred resolution order:

1. Put projection ownership in the boundary crate that serves the target shape.
2. For pure projection, prefer a local trait such as `IntoApiDto<T>`.
3. For context-dependent projection, keep one owned mapper function in the same projection module.
4. For shared enum string rules, move them to the enum's owning crate with `Display` and `FromStr`.
5. Delete the superseded local helper paths.

## Atomic Commit Rule

### Core Principle

**Every commit MUST represent exactly ONE engineering intent.**

A commit is invalid if it mixes:

- feature + refactor
- logic change + formatting
- bug fix + cleanup
- domain layer + infra/platform layer

If the commit message requires words like `and`, `also`, `plus`, `misc`, `update`, split the commit.

### Allowed Commit Types

- `feat:` new user-facing capability
- `impl:` concrete implementation step of a planned feature
- `fix:` bug fix
- `hotfix:` urgent production fix
- `refactor:` structural change without behavior change
- `arch:` architecture or boundary change
- `chore:` tooling, build, dependency, scripts
- `infra:` deployment or environment config
- `test:` add or adjust tests
- `perf:` performance optimization (benchmark required)
- `docs:` documentation only

### Pre-Commit Self Check

Before committing, verify:

1. This commit has exactly ONE clear goal.
2. Removing this commit removes only ONE capability/change.
3. The diff cannot be logically split.

If condition 3 is false, split the commit.

### Diff Scope Validation

Abort commit if diff contains:

- Domain logic + infrastructure implementation
- Port interface + adapter implementation
- Functional logic + formatting changes
- Multiple bounded contexts

Required split example:

```text
refactor: extract crypto utils module
feat: implement pairing handshake flow
```

### Hexagonal Architecture Commit Boundary Rule

The following MUST NOT appear in the same commit:

- `uc-core` + `uc-infra`
- Port definition + Adapter implementation
- App use-case + Platform integration

Required order:

```text
arch: add BlobRepository port
impl: implement sqlite BlobRepository adapter
```

### Commit Message Format

```text
<type>: <single intent summary>

[optional context]
```

### Revert Safety Rule

Every commit MUST satisfy:

- Project builds successfully
- Tests still pass (or explicitly documented breaking commit)
- No "half-prepared" commits for future steps

Never commit code that only exists to support a later commit.
