# Frontend and UI Rules

Use this document when editing React, TypeScript, Tailwind, UX flows, or frontend-facing daemon/Tauri integration.

## Frontend Layout Rules

- **No fixed-pixel layouts.**
  - Use **Tailwind utilities** or **rem** units.

## Theme Support Best Practices

Always verify components in both light and dark themes.

### Container Components

- Use `bg-card` + `text-card-foreground` for containers with content.
- Use `bg-background` only for page/base backgrounds.
- Use `bg-muted` for disabled/readonly states with `text-foreground`.

Examples:

```tsx
// ❌ Wrong
<DialogContent className="bg-background" />

// ✅ Correct
<DialogContent className="bg-card text-card-foreground" />

// ❌ Wrong
<input className="bg-muted text-muted-foreground" readOnly />

// ✅ Correct
<input className="bg-muted/50 text-foreground" readOnly />
```

### Status Messages

- Add `border border-{color}/20` to banners for better light-mode visibility.
- Use `font-medium` on status text when readability matters.
- Prefer `/70` over `/60` hover opacity when contrast is marginal.

## Frontend Architecture Notes

- Prefer API wrappers in `src/api/*` and shared helpers over direct `invoke()` in components.
- Keep route gating in `App.tsx` or layout-level logic, not duplicated in leaf components.
- Avoid parallel state sources for the same domain (local cache + Redux for the same truth).
- Match TypeScript DTO field names to actual Rust serde output. Do not assume global snake_case or camelCase consistency.

## Test Execution Note

For frontend unit tests involving Vitest mocks, fake timers, or jsdom, prefer `npx vitest run` over `bun test`.
