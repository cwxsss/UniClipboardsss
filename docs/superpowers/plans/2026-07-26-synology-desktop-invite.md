# Synology Desktop Invite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a desktop onboarding section to the Synology admin web page so users can generate a Space invitation code for desktop clients from the same container.

**Architecture:** The admin Node.js service owns the desktop invite process lifecycle. It starts `uniclip invite`, parses the first `INVITATION_CODE=...` line from stdout, returns the code immediately, and keeps the child process alive so the joiner can complete the handshake. The entrypoint optionally exposes the Space passphrase to the admin service only when `UC_ADMIN_SHOW_SPACE_PASSPHRASE=1`.

**Tech Stack:** POSIX shell entrypoint, Node.js built-in `http` and `child_process`, static HTML/CSS/JavaScript, existing `uniclip invite` / `uniclip join` CLI behavior.

## Global Constraints

- Do not reuse `mobile-sync` credentials for desktop onboarding.
- Do not persist invitation codes or passphrases to disk.
- Do not log the Space passphrase.
- Allow only one active desktop invite process per admin service instance.
- If `UC_ADMIN_SHOW_SPACE_PASSPHRASE` is not enabled, show a command template with `<你的空间口令>`.
- Keep the admin web page LAN-only; do not expose it publicly.

---

### Task 1: Admin Service Desktop Invite API

**Files:**
- Modify: `deploy/synology/admin-web/server.js`
- Test: `scripts/test-synology-server-wrapper.ps1`

**Interfaces:**
- Produces: `POST /api/desktop-invite` returning `{ code, expiresAtMs, command, passphraseIncluded }`.
- Consumes: `UC_SPACE_PASSPHRASE`, `UC_ADMIN_SHOW_SPACE_PASSPHRASE`, `UC_UNICLIP_BIN`.

- [ ] **Step 1: Add failing static checks**

Require these strings in `scripts/test-synology-server-wrapper.ps1`:

```powershell
Assert-Contains -Path $adminServer -Pattern "POST /api/desktop-invite"
Assert-Contains -Path $adminServer -Pattern "startDesktopInvite"
Assert-Contains -Path $adminServer -Pattern "INVITATION_CODE="
Assert-Contains -Path $adminServer -Pattern "UC_ADMIN_SHOW_SPACE_PASSPHRASE"
Assert-Contains -Path $adminServer -Pattern "cleanupDesktopInvite"
```

- [ ] **Step 2: Verify the checks fail**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1
```

Expected: fails because the desktop invite API is not implemented yet.

- [ ] **Step 3: Implement minimal API**

Add one active child process slot, parse `INVITATION_CODE=...`, return the join command, and clean up the child on exit or replacement.

- [ ] **Step 4: Verify checks pass**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1
node --check deploy/synology/admin-web/server.js
```

Expected: both pass.

### Task 2: Admin Web Desktop Invite UI

**Files:**
- Modify: `deploy/synology/admin-web/static/index.html`
- Modify: `deploy/synology/admin-web/static/app.js`
- Modify: `deploy/synology/admin-web/static/styles.css`
- Test: `scripts/test-synology-server-wrapper.ps1`

**Interfaces:**
- Consumes: `POST /api/desktop-invite`.
- Produces: a "desktop connection" panel that displays invitation code, join command, and passphrase handling note.

- [ ] **Step 1: Add failing static checks**

Require these strings in `scripts/test-synology-server-wrapper.ps1`:

```powershell
Assert-Contains -Path $adminIndex -Pattern "桌面端连接"
Assert-Contains -Path $adminApp -Pattern "/api/desktop-invite"
Assert-Contains -Path $adminApp -Pattern "desktopInviteCode"
Assert-Contains -Path $adminStyles -Pattern ".desktop-invite"
```

- [ ] **Step 2: Verify the checks fail**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1
```

Expected: fails because UI is not implemented yet.

- [ ] **Step 3: Implement UI**

Add the panel and JavaScript handlers. Keep the passphrase hidden unless the API reports `passphraseIncluded: true`.

- [ ] **Step 4: Verify checks pass**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1
```

Expected: passes.

### Task 3: Entrypoint Environment Wiring

**Files:**
- Modify: `deploy/synology/uniclipboard-server-entrypoint.sh`
- Test: `scripts/test-synology-server-wrapper.ps1`

**Interfaces:**
- Consumes: `UC_ADMIN_SHOW_SPACE_PASSPHRASE`.
- Produces: admin service environment values without forcing passphrase exposure by default.

- [ ] **Step 1: Add failing static checks**

Require these strings in `scripts/test-synology-server-wrapper.ps1`:

```powershell
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_SHOW_SPACE_PASSPHRASE"
Assert-Contains -Path $entrypoint -Pattern "UC_SPACE_PASSPHRASE=\"${UC_SPACE_PASSPHRASE}\""
```

- [ ] **Step 2: Verify the checks fail**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1
```

Expected: fails because the entrypoint does not pass this flag to admin web yet.

- [ ] **Step 3: Implement entrypoint wiring**

Read `UC_ADMIN_SHOW_SPACE_PASSPHRASE`, default it to `0`, and pass it to `uniclipboard-admin-web`. Pass `UC_SPACE_PASSPHRASE` through so the Node service can decide whether to include it in responses.

- [ ] **Step 4: Full verification**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1
node --check deploy/synology/admin-web/server.js
git diff --check
```

Expected: all pass.
