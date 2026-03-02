# Testing Patterns

**Analysis Date:** 2026-03-02

## Test Framework

**Frontend Runner:**

- Vitest 4.0.17 (configured in `vite.config.ts`)
- Config: `vite.config.ts` with `test` section
- Environment: jsdom (browser simulation)
- Globals: true (no need to import `describe`, `it`, `expect`)
- Setup file: `src/test/setup.ts`

**Backend (Rust) Runner:**

- Native Rust test framework (no external test framework)
- Test harness: built-in `cargo test` via `#[tokio::test]` and `#[test]` attributes
- Async runtime: Tokio (`tokio::test` macro for async tests)

**Run Commands:**

```bash
# Frontend
bun test                    # Run all tests
bun test --watch           # Watch mode
bun test --coverage        # Coverage report (via Vitest)

# Backend
cd src-tauri && cargo test                         # Run all Rust tests
cd src-tauri && cargo test --lib                   # Library tests only
cd src-tauri && cargo test --test '*'              # Integration tests only
cd src-tauri && cargo test --workspace             # All workspace crates
cd src-tauri && cargo llvm-cov --html --workspace # Coverage report
```

**Assertion Library:**

- Frontend: Vitest built-in (`expect()`)
- Backend: standard `assert!()` and `assert_eq!()` macros
- Frontend supplemental: `@testing-library/jest-dom` (e.g., `.toBeDisabled()`, `.toHaveTextContent()`)

## Test File Organization

**Location (Frontend):**

- Co-located with source files: `src/components/__tests__/` for components, `src/api/__tests__/` for APIs
- Test file naming: `[source].test.ts` or `[source].test.tsx`
- Example: `ClipboardItem.tsx` → `__tests__/ClipboardItem.test.tsx`

**Location (Rust):**

- Integration tests: `src-tauri/crates/[crate]/tests/[test].rs`
- Unit tests: inline in source files with `#[cfg(test)]` modules
- Example: `src-tauri/crates/uc-tauri/tests/integration_clipboard_capture.rs`

**Naming (Frontend):**

- Pattern: `[Feature].test.ts(x)` (e.g., `clipboardItems.test.ts`, `PairingDialog.test.tsx`)
- Group related tests in a single file for the same source module

**Naming (Rust):**

- Pattern: `[feature]_[type].rs` (e.g., `integration_clipboard_capture.rs`, `commands_test.rs`)
- Use descriptive names that indicate test scope

**Structure (Frontend):**

```
src/
├── components/
│   ├── ClipboardItem.tsx
│   └── __tests__/
│       └── ClipboardItem.test.tsx
├── api/
│   ├── clipboardItems.ts
│   └── __tests__/
│       └── clipboardItems.test.ts
└── hooks/
    ├── useShortcut.ts
    └── __tests__/
        └── useShortcut.test.ts
```

**Structure (Rust):**

```
src-tauri/crates/uc-tauri/
├── src/
│   ├── commands/
│   │   ├── clipboard.rs
│   │   └── mod.rs
│   └── lib.rs
└── tests/
    ├── integration_clipboard_capture.rs
    ├── commands_test.rs
    └── bootstrap_integration_test.rs
```

## Test Structure

**Frontend Suite Organization:**

```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

describe('ComponentName', () => {
  beforeEach(() => {
    // Setup before each test
    vi.clearAllMocks()
  })

  it('should render initial state', () => {
    render(<Component />)
    expect(screen.getByRole('button')).toBeInTheDocument()
  })

  it('should handle user interaction', async () => {
    const user = userEvent.setup()
    render(<Component />)

    const button = screen.getByRole('button')
    await user.click(button)

    expect(button).toBeDisabled()
  })
})
```

**Key Patterns:**

- `describe()`: Group related tests
- `beforeEach()`: Setup common test conditions
- `vi.clearAllMocks()`: Reset all mocks between tests
- `userEvent.setup()`: Create user interaction session
- `render()`: Mount React component in test environment
- `screen.getByRole()`, `screen.findByRole()`: Query by accessible role
- `act()`: Wrap state updates and async operations for proper batching

**Rust Test Organization:**

```rust
#[tokio::test]
async fn test_operation_succeeds() {
    // Arrange
    let handler = Arc::new(MockHandler::new());
    let snapshot = SystemClipboardSnapshot { /* ... */ };

    // Act
    let result = handler.on_clipboard_changed(snapshot).await;

    // Assert
    assert!(result.is_ok());
    assert!(handler.was_called());
}

#[test]
fn test_sync_operation() {
    // Synchronous test
    let value = compute_something();
    assert_eq!(value, expected);
}
```

**Key Patterns:**

- `#[tokio::test]`: Async test with Tokio runtime
- `#[test]`: Synchronous test
- Arrange-Act-Assert structure for clarity
- `Arc<T>` for shared state across async operations
- `assert!()`, `assert_eq!()` for simple checks
- `is_ok()`, `is_err()` for Result validation

## Mocking

**Framework (Frontend):** Vitest native `vi` module (no external library)

**Patterns:**

```typescript
// Define mocks at module level (hoisted)
const getClipboardItemsMock = vi.hoisted(() => vi.fn())
const deleteItemMock = vi.hoisted(() => vi.fn())

// Mock entire module
vi.mock('@/api/clipboardItems', () => ({
  getClipboardItems: getClipboardItemsMock,
  deleteClipboardItem: deleteItemMock,
}))

describe('Component', () => {
  beforeEach(() => {
    // Reset mock state
    getClipboardItemsMock.mockResolvedValue({ status: 'ready', items: [] })
    deleteItemMock.mockResolvedValue(true)
  })

  it('calls API on mount', async () => {
    render(<Component />)

    expect(getClipboardItemsMock).toHaveBeenCalled()
  })

  it('handles async responses', async () => {
    getClipboardItemsMock.mockResolvedValueOnce({
      status: 'ready',
      items: [{ id: '1', item: { text: 'test' } }]
    })

    render(<Component />)

    const item = await screen.findByText('test')
    expect(item).toBeInTheDocument()
  })
})
```

**What to Mock:**

- External API calls (`@/api/` functions)
- Tauri command invocations
- Context providers and Redux store (use fixture data)
- Heavy dependencies (filesystem, network)

**What NOT to Mock:**

- Native HTML elements
- React hooks themselves (test behavior, not implementation)
- Utility functions that are tested separately
- CSS classes (use `className` assertions with `expect().toHaveClass()`)

**Assertion Library (Frontend):**

- `expect(mock).toHaveBeenCalled()`
- `expect(mock).toHaveBeenCalledWith(arg1, arg2)`
- `expect(value).toBe()` for primitives
- `expect(value).toEqual()` for objects
- `expect(element).toBeInTheDocument()` (from jest-dom)
- `expect(element).toBeDisabled()` (from jest-dom)
- `expect(element).toHaveTextContent('text')` (from jest-dom)

**Mocking (Rust) - Mock Structures:**

```rust
// Define mock struct implementing trait
struct MockHandler {
    capture_called: Arc<AtomicBool>,
    snapshot_received: Arc<Mutex<Option<SystemClipboardSnapshot>>>,
}

impl MockHandler {
    fn new() -> Self {
        Self {
            capture_called: Arc::new(AtomicBool::new(false)),
            snapshot_received: Arc::new(Mutex::new(None)),
        }
    }

    fn was_called(&self) -> bool {
        self.capture_called.load(Ordering::SeqCst)
    }
}

// Implement trait being tested
#[async_trait::async_trait]
impl ClipboardChangeHandler for MockHandler {
    async fn on_clipboard_changed(&self, snapshot: SystemClipboardSnapshot) -> anyhow::Result<()> {
        self.capture_called.store(true, Ordering::SeqCst);
        *self.snapshot_received.lock().unwrap() = Some(snapshot);
        Ok(())
    }
}

#[tokio::test]
async fn test_handler_callback() {
    let handler = Arc::new(MockHandler::new());
    let handler_inner = handler.clone();

    // Test the trait implementation
    handler.on_clipboard_changed(snapshot).await.unwrap();

    assert!(handler_inner.was_called());
}
```

**Assertion Patterns (Rust):**

- `assert!(condition, "message")` - boolean check
- `assert_eq!(actual, expected, "message")` - equality check
- `result.is_ok()` / `result.is_err()` - Result checks
- `option.is_some()` / `option.is_none()` - Option checks

## Fixtures and Factories

**Test Data (Frontend):**

- Store mock responses as constants in test files
- Example in `clipboardItems.test.ts`:

  ```typescript
  const mockEntry = {
    id: 'entry-1',
    preview: 'test content',
    has_detail: true,
    size_bytes: 100,
    captured_at: 1,
    content_type: 'text/plain',
    is_encrypted: false,
    is_favorited: false,
    updated_at: 1,
    active_time: 1,
  }

  invokeMock.mockResolvedValueOnce({
    status: 'ready',
    entries: [mockEntry],
  })
  ```

**Test Data (Rust):**

- Use builder pattern or factory functions for complex test objects
- Example in `integration_clipboard_capture.rs`:
  ```rust
  let snapshot = SystemClipboardSnapshot {
    ts_ms: 12345,
    representations: vec![ObservedClipboardRepresentation {
      id: RepresentationId::from("test-rep-1".to_string()),
      format_id: FormatId::from("public.utf8-plain-text".to_string()),
      mime: Some(MimeType("text/plain".to_string())),
      bytes: vec![b'H', b'e', b'l', b'l', b'o'],
    }],
  };
  ```

**Location:**

- Frontend: Inline in test file or `src/test/fixtures/` if reused
- Rust: Inline in test file or test utility module

## Coverage

**Frontend Requirements:**

- Target: Not formally enforced, but aim for >70% on critical paths
- View coverage: `bun test --coverage` (Vitest coverage)
- Focus areas: API layer, critical hooks, component behavior

**Rust Requirements:**

- No formal target enforced
- View coverage: `cd src-tauri && cargo llvm-cov --html --workspace`
- Then open: `src-tauri/target/llvm-cov/html/index.html`

**Coverage Exclusions (Rust):**

- Test code itself
- Platform-specific code that can't be tested on current OS
- Error paths that are infrastructure-level (impossible to trigger in tests)

## Test Types

**Unit Tests (Frontend):**

- Scope: Single function or component in isolation
- Dependencies: Mocked
- Approach: Test inputs → outputs, no real API calls
- Example: `clipboardItems.test.ts` tests data transformation functions
- Location: Co-located in `__tests__/` directory

**Unit Tests (Rust):**

- Scope: Single function/struct method
- Dependencies: None (or mocked)
- Approach: Direct function calls with test values
- Example: `test_update_settings_validates_schema_version()` tests validation logic
- Location: Inline with `#[cfg(test)]` or in `tests/` directory

**Integration Tests (Frontend):**

- Scope: Component + mocked API interactions
- Dependencies: Mocked APIs
- Approach: User interactions → state changes → UI updates
- Example: `PairingDialog.test.tsx` tests user flow through dialog
- Location: Same `__tests__/` directory as unit tests

**Integration Tests (Rust):**

- Scope: Use case execution → repository → database
- Dependencies: Real or test doubles with state
- Approach: Full flow from command to persistence
- Example: `integration_settings.rs` tests GetSettings/UpdateSettings use cases with temp file storage
- Location: `tests/` directory with `#[tokio::test]` async tests

**E2E Tests:**

- Framework: Not currently configured
- Recommendation: Could be added with Tauri test harness if needed for full app flow testing

## Common Patterns

**Async Testing (Frontend):**

```typescript
// Wait for async operations to complete
const item = await screen.findByText('expected text')

// Use act() for state updates
await act(async () => {
  handler?.({ kind: 'verification', sessionId: 'session-1' })
})

// Use user interaction API
const user = userEvent.setup()
await user.click(button)
await user.type(input, 'text')

// Handle promise-based mocks
mockFn.mockResolvedValueOnce({ status: 'ready', items: [] })
mockFn.mockResolvedValue({ status: 'not_ready' })
mockFn.mockRejectedValueOnce(new Error('failed'))
```

**Async Testing (Rust):**

```rust
#[tokio::test]
async fn test_async_handler() {
    let handler = Arc::new(MockHandler::new());
    let snapshot = SystemClipboardSnapshot { /* ... */ };

    // Call async trait method
    let result = handler.on_clipboard_changed(snapshot).await;

    // Assert result
    assert!(result.is_ok());
}

// For concurrent testing
#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_operations() {
    // Test with multiple threads
}
```

**Error Testing (Frontend):**

```typescript
it('handles API errors gracefully', async () => {
  getClipboardItemsMock.mockRejectedValueOnce(new Error('Network error'))

  render(<Component />)

  // Error is handled by component
  const errorElement = await screen.findByText(/error|failed/i)
  expect(errorElement).toBeInTheDocument()
})
```

**Error Testing (Rust):**

```rust
#[tokio::test]
async fn test_update_settings_validates_schema() {
    let mut settings = Settings::default();
    settings.schema_version = 999; // Invalid

    let update_uc = UpdateSettings::new(repo);
    let result = update_uc.execute(settings).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid schema version"));
}
```

**Component Testing Best Practices:**

- Always test user interactions, not implementation details
- Use `screen.getByRole()` for accessible queries (more robust than `querySelector`)
- Avoid testing internal state; test side effects instead
- Example bad: `expect(component.state.isLoading).toBe(false)`
- Example good: `expect(screen.queryByText('Loading...')).not.toBeInTheDocument()`

## Setup and Configuration

**Frontend Setup File (`src/test/setup.ts`):**

- Imports `@testing-library/jest-dom/vitest` for extended matchers
- Stubs environment variables (e.g., `VITE_SENTRY_DSN`)
- Implements localStorage polyfill for jsdom
- Initializes i18n module for translation tests

**Vite Config Integration:**

```typescript
test: {
  environment: 'jsdom',
  globals: true,
  setupFiles: './src/test/setup.ts',
  exclude: ['**/node_modules/**', '**/dist/**', '**/.worktrees/**'],
}
```

**Rust Test Configuration:**

- Uses `#[tokio::test]` for async runtime setup
- Uses `tempfile` crate for temporary directories in integration tests
- Example: `integration_settings.rs` creates temp directory for settings file

## Running Specific Tests

**Frontend:**

```bash
# Run single test file
bun test src/components/__tests__/ClipboardItem.test.tsx

# Run tests matching pattern
bun test --grep "clipboard"

# Watch mode for specific file
bun test --watch src/api/__tests__/
```

**Rust:**

```bash
# Run single test
cd src-tauri && cargo test test_clipboard_change_handler_receives_callback

# Run tests in specific crate
cd src-tauri && cargo test --package uc-tauri

# Run integration tests only
cd src-tauri && cargo test --test integration_clipboard_capture

# Run with output
cd src-tauri && cargo test -- --nocapture
```

---

_Testing analysis: 2026-03-02_
