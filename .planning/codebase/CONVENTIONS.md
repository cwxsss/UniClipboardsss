# Coding Conventions

**Analysis Date:** 2026-03-02

## Naming Patterns

**Files:**

- TypeScript/React: `camelCase.ts`, `camelCase.tsx` (e.g., `clipboardItems.ts`, `ClipboardItem.tsx`)
- React components: PascalCase for component files only (e.g., `PairingDialog.tsx`)
- Rust: `snake_case.rs` (e.g., `model.rs`, `integration_clipboard_capture.rs`)
- Directories: kebab-case for feature grouping (e.g., `src/components/clipboard/`, `src-tauri/crates/uc-core/`)

**Functions:**

- TypeScript: `camelCase` for regular functions and methods
  - Example: `getClipboardItems()`, `deleteClipboardItem()`, `invokeWithTrace()`
- React hooks: `useCamelCase` prefix mandatory (e.g., `useShortcut()`, `useSetting()`, `useUpdate()`)
- Rust: `snake_case` for all functions (e.g., `get_clipboard_entries()`, `should_return_not_ready()`)

**Variables:**

- TypeScript: `camelCase` (e.g., `isExpanded`, `detailContent`, `entryId`)
- Rust: `snake_case` (e.g., `resolved_limit`, `session_ready`, `device_id`)
- Constants: SCREAMING_SNAKE_CASE in Rust (e.g., `CURRENT_SCHEMA_VERSION`), camelCase in TypeScript for non-exported, UPPER_CASE for exported constants
- Enums: PascalCase variants with snake_case serialization in Rust (e.g., `UpdateChannel::Stable` serializes as `"stable"`)

**Types:**

- TypeScript: PascalCase for interfaces and type aliases (e.g., `ClipboardItemResponse`, `ClipboardEntriesResponse`)
- Rust: PascalCase for structs and enums (e.g., `GeneralSettings`, `Theme`, `EncryptionState`)

## Code Style

**Formatting:**

- Prettier (TypeScript/React): [`.prettierrc`](/.prettierrc)
  - Semi colons: disabled (`"semi": false`)
  - Single quotes: enabled (`"singleQuote": true`)
  - Tab width: 2 spaces
  - Line width: 100 characters
  - Trailing commas: ES5 style
  - Arrow parentheses: avoided where possible (`"arrowParens": "avoid"`)
  - Line endings: LF only

- Rust: standard `cargo fmt` (no custom `.rustfmt.toml`)
  - Run via: `cargo fmt --manifest-path=src-tauri/Cargo.toml`

**Linting:**

- ESLint (`eslint.config.js`):
  - TypeScript strict rules enabled
  - React plugin with hooks validation
  - React Refresh warnings for export patterns
  - Import ordering enforced via `eslint-plugin-import-x`:
    - Groups: builtin → external → internal → parent → sibling → index
    - Alphabetical ordering within groups
    - No newlines between groups (`"newlines-between": "never"`)
  - Unused variables: error, with `^_` pattern for intentionally ignored params
  - JSDoc/TSDoc not strictly enforced but recommended

## Import Organization

**Order:**

1. Node.js built-ins (`import ... from 'node:path'`)
2. External packages (`import React from 'react'`, `import { invoke } from '@tauri-apps/api/core'`)
3. Internal absolute imports (via `@/` alias: `import { cn } from '@/lib/utils'`)
4. Relative imports (parent, sibling, index)
5. Side effects (`import '@testing-library/jest-dom/vitest'`)

**Path Aliases:**

- `@/*` → `src/*` (configured in `tsconfig.json`)
- Prefer absolute imports over relative for cross-cutting utilities
- Example: `import { invokeWithTrace } from '@/lib/tauri-command'` instead of `../../../lib/tauri-command`

## Error Handling

**TypeScript/Frontend:**

- Use try-catch for async operations, throw errors for propagation
- Always log errors to console (e.g., `console.error('Failed to fetch:', error)`)
- Emit user-facing errors via toast notifications: `toast.error(t('...'), { description: errorMessage })`
- Type errors as `error instanceof Error` for safe message extraction
- Example pattern:
  ```typescript
  export async function getClipboardItems(): Promise<ClipboardItemsResult> {
    try {
      return await invokeWithTrace<ClipboardEntriesResponse>('get_clipboard_entries', {...})
    } catch (error) {
      console.error('获取剪贴板历史记录失败:', error)
      throw error  // Propagate to caller
    }
  }
  ```

**Rust (Critical):**

- **NEVER use `unwrap()` or `expect()` in production code** (only acceptable in tests)
- Always use explicit error handling:
  - Pattern matching with `match` for errors that must be reported
  - `?` operator with proper error conversion for error propagation
  - `unwrap_or_default()` / `unwrap_or_else()` for non-critical defaults
- Example:

  ```rust
  // ❌ FORBIDDEN
  let value = some_option.unwrap();

  // ✅ CORRECT - Pattern matching
  match some_result {
    Ok(value) => { /* handle */ },
    Err(e) => {
      error!("Operation failed: {}", e);
      return Err(e);
    }
  }

  // ✅ CORRECT - ? operator
  pub fn do_something() -> Result<(), MyError> {
    let value = some_option.ok_or(MyError::NotFound)?;
    Ok(())
  }
  ```

**Event-Driven Code (Rust):**

- Never silently ignore errors in event handlers
- Use explicit `match` instead of `if let Ok(...)`
- Always log errors with `warn!()` or `error!()`
- Emit error events for frontend notification
- Example:

  ```rust
  // ❌ WRONG - Silent failure
  NetworkCommand::SendRequest { data } => {
    if let Ok(peer) = parse_peer(&data) {
      self.send(peer);
    }
    // Error is swallowed - user gets no feedback!
  }

  // ✅ CORRECT - Explicit error handling
  match parse_peer(&data) {
    Ok(peer) => { self.send(peer); }
    Err(e) => {
      warn!("Invalid peer format: {}", e);
      let _ = self.event_tx.send(NetworkEvent::Error(
        format!("Failed to send: {}", e)
      )).await;
    }
  }
  ```

## Logging

**Framework:**

- Rust backend: `tracing` crate with structured logging and spans
- Frontend: `console.error()`, `console.warn()`, `console.log()` (legacy)
- Sentry integration for error tracking via `Sentry.captureException()`

**Rust Patterns:**

- Spans for operations with duration: `info_span!("operation.name", param = %value)`
- Events for point-in-time occurrences: `info!()`, `error!()`, `warn!()`, `debug!()`
- Use `#[tracing::instrument]` macro on functions to auto-create spans
- Span fields: use `%` prefix for Display types, no prefix for Debug
- Example:
  ```rust
  pub async fn list_entries() {
    let span = info_span!("clipboard.list", limit = 50);
    async {
      info!("Starting to list entries");
      // ... operation ...
      info!(count = 5, "Entries retrieved");
    }
    .instrument(span)
    .await
  }
  ```

**TypeScript Patterns:**

- Use `invokeWithTrace()` wrapper for all Tauri commands
- Wrapper automatically:
  - Creates trace IDs with timestamps
  - Redacts sensitive arguments before logging
  - Adds Sentry breadcrumbs
  - Captures exceptions in Sentry on errors
- Example: `await invokeWithTrace('get_clipboard_items', { limit: 50 })`

## Comments

**When to Comment:**

- Complex algorithms or non-obvious logic
- Implementation decisions that differ from requirements
- TODO/FIXME with context on what needs fixing and why (e.g., `// TODO: Implement proper content type detection when backend provides accurate values`)
- Type clarifications in interfaces (especially for optional fields)
- Bilingual comments acceptable (English for code, Chinese for business context)

**JSDoc/TSDoc:**

- Optional but recommended for public APIs
- Parameters: Document with `@param` and type
- Return values: Document with `@returns` and type
- Example:
  ```typescript
  /**
   * 获取剪贴板历史记录
   * @param limit 限制返回的条目数
   * @returns Promise，返回剪贴板条目数组
   */
  export async function getClipboardItems(limit?: number): Promise<ClipboardItemsResult>
  ```

**Rust Doc Comments:**

- Use `///` for public items (functions, structs, modules)
- Use `//!` for module-level documentation
- Example:
  ```rust
  /// Get clipboard history entries (preview only)
  /// 获取剪贴板历史条目（仅预览）
  #[tauri::command]
  pub async fn get_clipboard_entries(
    runtime: State<'_, Arc<AppRuntime>>,
  ) -> Result<ClipboardEntriesResponse, String>
  ```

## Function Design

**Size:**

- Keep functions under ~50 lines for readability
- Extract complex conditional logic into helper functions
- Use descriptive helper function names to document intent (e.g., `should_return_not_ready()`, `isImageType()`)

**Parameters:**

- Prefer explicit parameters over implicit context
- Use type-safe enums for options instead of string unions where possible
- Limit to 4-5 parameters; use object parameter for more
- Example with object param:
  ```typescript
  export async function getClipboardItems(
    _orderBy?: OrderBy,
    limit?: number,
    offset?: number,
    _filter?: Filter
  ): Promise<ClipboardItemsResult>
  ```

**Return Values:**

- Use discriminated unions (TypeScript) for multi-state returns
- Example: `{ status: 'ready'; items: [...] } | { status: 'not_ready' }`
- Use `Result<T, E>` in Rust for fallible operations
- Rust: Prefer `anyhow::Result<T>` with context-specific error messages

## Module Design

**Exports:**

- TypeScript: Use named exports, default exports only for components
- Example: `export { useShortcut }`, `export default ClipboardItem`
- Barrel files: `index.ts` at directory level to re-export public APIs
- Example in `src/api/index.ts`: `export { getClipboardItems, deleteClipboardItem } from './clipboardItems'`

**Barrel Files:**

- Keep barrel files minimal: only export public APIs, not implementation details
- Use for grouping related exports by feature/domain
- Reduce import complexity for consumers

**Rust Module Organization:**

- Use `mod.rs` files to organize sub-modules and public API
- Private modules: prefix with `mod` keyword, not included in `pub use`
- Public APIs: explicitly `pub use` from `mod.rs`
- Example structure:

  ```rust
  // src/commands/mod.rs
  pub mod clipboard;
  pub mod settings;

  pub use clipboard::{get_clipboard_entries, delete_clipboard_item};
  pub use settings::{get_settings, update_settings};
  ```

## TypeScript Strict Mode

**Enabled features:**

- No `any` types (strict rule)
- No unused locals
- No unused parameters (with `^_` pattern for intentional ignores)
- No fallthrough cases in switch
- Strict null checks

**Patterns:**

- Use `as const` for literal type inference where needed
- Use type guards for safe type narrowing
- Use `satisfies` operator for type validation without type assertion
- Example:
  ```typescript
  const displayType = (item.text ? 'text' : 'image') as const
  ```

## Async/Await Patterns

**Frontend (React):**

- Use async/await in event handlers, prefer `void handleClick = async () => {}`
- Always catch errors in try-catch blocks
- Use `useEffect` for side effects, avoid async callbacks directly
- Example:
  ```tsx
  const handleExpand = async () => {
    setIsLoadingDetail(true)
    try {
      const resource = await getClipboardEntryResource(entryId)
      setDetailContent(resource.url)
    } catch (e) {
      console.error('Failed to load:', e)
      toast.error(t('errors.loadFailed'))
    } finally {
      setIsLoadingDetail(false)
    }
  }
  ```

**Rust:**

- Use `#[tokio::test]` for async tests
- Use `.instrument(span).await` for tracing async operations
- Use `Arc<dyn Trait>` for shared trait objects across async boundaries
- Example:
  ```rust
  #[tokio::test]
  async fn test_something() {
    let handler = Arc::new(MockHandler::new());
    handler.on_clipboard_changed(snapshot).await.unwrap();
  }
  ```

---

_Convention analysis: 2026-03-02_
