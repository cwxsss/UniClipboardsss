# Changelog Template

本模板用于每次发布时生成 changelog，基于 conventional commits 分类。

## 模板

```markdown
## {VERSION} - {YYYY-MM-DD}

### Features

- {描述新增功能，每条一行}

### Fixes

- {描述修复的问题，每条一行}

### Refactor

- {描述重构变更，每条一行}

### Performance

- {描述性能优化，每条一行}

### Tests

- {描述测试变更，每条一行}

### Docs

- {描述文档变更，每条一行}
```

## 规则

1. **仅包含有内容的分类**，空分类整段省略
2. **分类对应 conventional commit type**：
   - `feat:` → Features
   - `fix:` → Fixes
   - `refactor:` → Refactor
   - `perf:` → Performance
   - `test:` → Tests
   - `docs:` → Docs
   - `chore:` → 不列入 changelog
3. **每条描述用英文书写**，简洁说明变更内容
4. **Breaking Changes** 存在时，在最前面添加 `### Breaking Changes` 分类
5. **版本号**取自 `package.json` / `Cargo.toml` / `tauri.conf.json`
6. **日期**使用发布当天，格式 `YYYY-MM-DD`

## 示例

```markdown
## 0.2.0-alpha.3 - 2026-03-08

### Features

- Add `getClipboardEntry` API and `useClipboardEvents` hook for real-time clipboard event handling
- Add `prependItem`/`removeItem` reducers and `origin` field to Clipboard slice

### Fixes

- Use preview representation size instead of total entry size for display

### Refactor

- Simplify `DashboardPage` to thin render layer, delegating logic to hooks
```
