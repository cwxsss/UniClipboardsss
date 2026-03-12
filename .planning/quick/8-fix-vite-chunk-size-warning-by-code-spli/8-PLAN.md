---
phase: quick
plan: 8
type: execute
wave: 1
depends_on: []
files_modified:
  - vite.config.ts
  - src/App.tsx
autonomous: true
must_haves:
  truths:
    - 'Production build emits no chunk size warnings'
    - 'All pages still load and render correctly'
  artifacts:
    - path: 'vite.config.ts'
      provides: 'manualChunks configuration and route-level code splitting support'
    - path: 'src/App.tsx'
      provides: 'Lazy-loaded route components with React.lazy + Suspense'
  key_links:
    - from: 'vite.config.ts'
      to: 'rollup output'
      via: 'manualChunks splitting vendor libs'
      pattern: 'manualChunks'
    - from: 'src/App.tsx'
      to: 'pages/*'
      via: 'React.lazy dynamic import'
      pattern: "lazy\\("
---

<objective>
Fix the Vite chunk size warning by splitting the single 1,317 kB index bundle into smaller chunks.

Purpose: Eliminate the build warning and improve initial load performance by separating vendor libraries and lazy-loading routes.
Output: Clean production build with no chunk size warnings, multiple smaller chunks.
</objective>

<execution_context>
@/home/wuy6/.claude/get-shit-done/workflows/execute-plan.md
@/home/wuy6/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@vite.config.ts
@src/App.tsx
@package.json

Current state: `bun run build` produces a single 1,317 kB JS chunk triggering Vite's 500 kB warning.

Largest vendor dependencies (by node_modules size): lucide-react (46MB), @sentry (15M), @reduxjs (7.4M), @radix-ui (6.9M), framer-motion (3.3M).
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add manualChunks vendor splitting and lazy-load routes</name>
  <files>vite.config.ts, src/App.tsx</files>
  <action>
1. In `vite.config.ts`, add `build.rollupOptions.output.manualChunks` to split vendor libraries into separate chunks:
   - `vendor-react`: react, react-dom, react-router-dom
   - `vendor-redux`: @reduxjs/toolkit, react-redux
   - `vendor-radix`: all @radix-ui/* packages
   - `vendor-ui`: framer-motion, lucide-react, sonner
   - `vendor-sentry`: @sentry/react
   - `vendor-i18n`: i18next, react-i18next
   - `vendor-tauri`: all @tauri-apps/* packages

Use a function-based manualChunks that checks `id.includes('node_modules/...')` for each group.

2. In `src/App.tsx`, convert page imports to React.lazy:
   - `const DashboardPage = lazy(() => import('@/pages/DashboardPage'))`
   - `const DevicesPage = lazy(() => import('@/pages/DevicesPage'))`
   - `const SettingsPage = lazy(() => import('@/pages/SettingsPage'))`
   - `const SetupPage = lazy(() => import('@/pages/SetupPage'))`
   - `const UnlockPage = lazy(() => import('@/pages/UnlockPage'))`

   Add `import { lazy, Suspense, ... } from 'react'` (add lazy and Suspense to existing import).

   Wrap the `<Routes>` block in AppContent with `<Suspense fallback={null}>`. Also wrap the conditional renders of `<SetupPage>` and `<UnlockPage>` with `<Suspense fallback={null}>`.

   Note: `fallback={null}` is appropriate here because the app already shows loading states (encryption loading returns null, setup gate loading). A blank flash is acceptable for these fast local route transitions in a desktop app.
   </action>
   <verify>
   <automated>cd /home/wuy6/myprojects/UniClipboard && bun run build 2>&1 | grep -c "chunks are larger than" | grep -q "0" && echo "PASS: No chunk size warnings" || echo "FAIL: Chunk size warning still present"</automated>
   </verify>
   <done>

- `bun run build` completes with zero chunk size warnings
- Build output shows multiple JS chunks instead of a single 1.3MB file
- No individual chunk exceeds 500 kB
  </done>
  </task>

<task type="auto">
  <name>Task 2: Verify application still works with split chunks</name>
  <files></files>
  <action>
1. Run `bun run build` and confirm:
   - Build succeeds without errors
   - Multiple JS chunks are produced (at least 4-5 separate .js files in dist/assets/)
   - No single chunk exceeds 500 kB
   - Total bundle size is roughly the same (splitting doesn't increase total significantly)

2. Run existing frontend tests to ensure no regressions:
   - `bun run test` (if configured) or `bunx vitest run` to run any existing test suite
   - Verify no import errors from the lazy loading changes

3. If any chunk still exceeds 500 kB, adjust the manualChunks grouping by further splitting the largest group.
   </action>
   <verify>
   <automated>cd /home/wuy6/myprojects/UniClipboard && bun run build 2>&1 | tail -20 && echo "---" && ls -la dist/assets/\*.js 2>/dev/null | wc -l && echo "JS chunk files produced"</automated>
   </verify>
   <done>

- Build produces 5+ separate JS chunk files
- No chunk exceeds 500 kB (no warning from Vite)
- All existing tests pass
  </done>
  </task>

</tasks>

<verification>
- `bun run build` produces no "chunks are larger than 500 kB" warning
- Multiple chunk files exist in dist/assets/
- Application compiles without TypeScript errors
</verification>

<success_criteria>

- Zero Vite chunk size warnings on production build
- Route-based code splitting active (pages loaded lazily)
- Vendor libraries split into logical groups
- No functional regressions
  </success_criteria>

<output>
After completion, create `.planning/quick/8-fix-vite-chunk-size-warning-by-code-spli/8-SUMMARY.md`
</output>
