# Synology Admin Web Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Synology Docker wrapper 中增加可选管理 Web 页，用独立端口生成移动端连接二维码、查看设备、撤销设备和轮换密码。

**Architecture:** 新增一个 Synology 专用 Node.js 管理代理，不改 UniClipboard daemon 的业务层。管理代理监听 `UC_ADMIN_PORT`，使用 `UC_ADMIN_PASSWORD` 登录态保护浏览器接口，通过 `uniclip mobile ... --json` 调用现有 daemon 能力。Docker entrypoint 在 `UC_ADMIN_WEB=1` 时启动管理代理，并让现有 daemon 继续作为容器主进程。

**Tech Stack:** POSIX shell entrypoint、Node.js 内置 `http`/`crypto`/`child_process` 模块、静态 HTML/CSS/JS、PowerShell wrapper 静态回归测试。

## Global Constraints

- `UC_ADMIN_WEB` 未启用时，现有镜像行为保持不变。
- `UC_ADMIN_WEB=1` 时必须设置非空 `UC_ADMIN_PASSWORD`。
- 管理服务默认监听 `0.0.0.0:${UC_ADMIN_PORT}`，默认端口为 `42888`。
- 管理服务不得直接访问 SQLite、剪贴板历史、剪贴板正文或文件内容。
- 管理服务不得在日志中打印管理密码、移动端一次性密码、Cookie 或完整连接 URI。
- 二维码必须使用 daemon/CLI 返回的 PNG base64，不依赖字符二维码。
- 新增代码注释使用英文，项目文档使用中文。

---

### Task 1: Wrapper Config And Process Lifecycle

**Files:**
- Modify: `deploy/synology/uniclipboard-server-entrypoint.sh`
- Modify: `scripts/test-synology-server-wrapper.ps1`

**Interfaces:**
- Consumes: environment variables `UC_ADMIN_WEB`, `UC_ADMIN_PORT`, `UC_ADMIN_PASSWORD`.
- Produces: entrypoint starts `/usr/local/bin/uniclipboard-admin-web` in background when admin web is enabled.

- [ ] **Step 1: Write the failing test**

Add assertions to `scripts/test-synology-server-wrapper.ps1`:

```powershell
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_WEB"
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_PORT"
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_PASSWORD"
Assert-Contains -Path $entrypoint -Pattern "admin_web_enabled()"
Assert-Contains -Path $entrypoint -Pattern "UC_ADMIN_WEB=1 requires UC_ADMIN_PASSWORD"
Assert-Contains -Path $entrypoint -Pattern "start_admin_web()"
Assert-Contains -Path $entrypoint -Pattern "uniclipboard-admin-web &"
Assert-Contains -Path $entrypoint -Pattern "ADMIN_WEB_PID="
Assert-Contains -Path $entrypoint -Pattern "trap stop_admin_web INT TERM EXIT"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1`

Expected: FAIL because the entrypoint does not contain `UC_ADMIN_WEB` and admin web lifecycle functions.

- [ ] **Step 3: Write minimal implementation**

Modify `deploy/synology/uniclipboard-server-entrypoint.sh`:

```sh
UC_ADMIN_WEB="$(strip_outer_quotes "${UC_ADMIN_WEB:-0}")"
UC_ADMIN_PORT="$(strip_outer_quotes "${UC_ADMIN_PORT:-42888}")"
UC_ADMIN_PASSWORD="$(strip_outer_quotes "${UC_ADMIN_PASSWORD:-}")"
ADMIN_WEB_PID=""

admin_web_enabled() {
  case "${UC_ADMIN_WEB}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

stop_admin_web() {
  if [ -n "${ADMIN_WEB_PID:-}" ]; then
    kill "${ADMIN_WEB_PID}" 2>/dev/null || true
    wait "${ADMIN_WEB_PID}" 2>/dev/null || true
  fi
}

start_admin_web() {
  if [ -z "${UC_ADMIN_PASSWORD:-}" ]; then
    echo "UC_ADMIN_WEB=1 requires UC_ADMIN_PASSWORD" >&2
    exit 1
  fi
  uniclipboard-admin-web &
  ADMIN_WEB_PID="$!"
  trap stop_admin_web INT TERM EXIT
}
```

Call `start_admin_web` after setup checks and mobile network configuration, before `uniclip stop`.

- [ ] **Step 4: Run test to verify it passes**

Run: `powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1`

Expected: PASS with `Synology server wrapper checks passed.`

- [ ] **Step 5: Commit**

```bash
git add deploy/synology/uniclipboard-server-entrypoint.sh scripts/test-synology-server-wrapper.ps1
git commit -m "infra: wire Synology admin web lifecycle"
```

### Task 2: Admin Web Server API

**Files:**
- Create: `deploy/synology/admin-web/server.js`
- Create: `deploy/synology/admin-web/static/index.html`
- Create: `deploy/synology/admin-web/static/app.js`
- Create: `deploy/synology/admin-web/static/styles.css`
- Modify: `scripts/test-synology-server-wrapper.ps1`

**Interfaces:**
- Consumes: `UC_ADMIN_PORT`, `UC_ADMIN_PASSWORD`.
- Produces: HTTP endpoints `POST /api/login`, `POST /api/logout`, `GET /api/status`, `GET /api/devices`, `POST /api/devices`, `DELETE /api/devices/{deviceId}`, `POST /api/devices/{deviceId}/rotate-password`.

- [ ] **Step 1: Write the failing test**

Add file existence and static contract checks to `scripts/test-synology-server-wrapper.ps1`:

```powershell
$adminServer = Join-Path $root "deploy/synology/admin-web/server.js"
$adminIndex = Join-Path $root "deploy/synology/admin-web/static/index.html"
$adminApp = Join-Path $root "deploy/synology/admin-web/static/app.js"
$adminStyles = Join-Path $root "deploy/synology/admin-web/static/styles.css"

foreach ($path in @($adminServer, $adminIndex, $adminApp, $adminStyles)) {
    if (-not (Test-Path $path)) {
        throw "Missing Synology admin web file: $path"
    }
}

Assert-Contains -Path $adminServer -Pattern "POST /api/login"
Assert-Contains -Path $adminServer -Pattern "GET /api/status"
Assert-Contains -Path $adminServer -Pattern "GET /api/devices"
Assert-Contains -Path $adminServer -Pattern "POST /api/devices"
Assert-Contains -Path $adminServer -Pattern "DELETE /api/devices/"
Assert-Contains -Path $adminServer -Pattern "rotate-password"
Assert-Contains -Path $adminServer -Pattern "HttpOnly"
Assert-Contains -Path $adminServer -Pattern "SameSite=Strict"
Assert-Contains -Path $adminServer -Pattern "password (redacted)"
Assert-Contains -Path $adminApp -Pattern "qrCodePngBase64"
Assert-Contains -Path $adminApp -Pattern "installQrCodePngBase64"
Assert-Contains -Path $adminIndex -Pattern "UniClipboard 管理"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1`

Expected: FAIL because admin web files do not exist.

- [ ] **Step 3: Write minimal implementation**

Create `deploy/synology/admin-web/server.js` with:

```js
#!/usr/bin/env node
'use strict';

const crypto = require('crypto');
const fs = require('fs');
const http = require('http');
const path = require('path');
const { execFile } = require('child_process');
```

Core behavior:

- `POST /api/login` validates `password` against `UC_ADMIN_PASSWORD` using `crypto.timingSafeEqual`, then returns `Set-Cookie: uc_admin_session=<id>; Path=/; HttpOnly; SameSite=Strict`.
- Protected API checks in-memory session map.
- `runUniclip(args)` executes `uniclip` with JSON commands and parses stdout JSON.
- Device creation calls `uniclip mobile add --label <label> --json` plus optional `--username` and `--password-stdin`.
- Device list calls `uniclip mobile status --json` or `uniclip mobile devices --json` depending on the existing CLI subcommand support verified in code.
- Device deletion calls `uniclip mobile revoke <deviceId> --json`.
- Password rotation calls the existing CLI device update or rotate command if available; if no CLI command exists, add the endpoint only after adding CLI support in a later task.
- Never log request bodies or command stdout.

- [ ] **Step 4: Run Node syntax check**

Run: `node --check deploy/synology/admin-web/server.js`

Expected: PASS with no output.

- [ ] **Step 5: Run wrapper test**

Run: `powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1`

Expected: PASS with `Synology server wrapper checks passed.`

- [ ] **Step 6: Commit**

```bash
git add deploy/synology/admin-web scripts/test-synology-server-wrapper.ps1
git commit -m "feat: add Synology admin web service"
```

### Task 3: Docker Image Integration

**Files:**
- Modify: `deploy/synology/Dockerfile`
- Modify: `scripts/test-synology-server-wrapper.ps1`

**Interfaces:**
- Consumes: admin web files from Task 2.
- Produces: Docker image contains Node.js, `uniclipboard-admin-web`, and static assets.

- [ ] **Step 1: Write the failing test**

Add Dockerfile assertions:

```powershell
Assert-Contains -Path $dockerfile -Pattern "apk add --no-cache nodejs"
Assert-Contains -Path $dockerfile -Pattern "COPY deploy/synology/admin-web/"
Assert-Contains -Path $dockerfile -Pattern "/opt/uniclipboard-admin-web/"
Assert-Contains -Path $dockerfile -Pattern "ln -s /opt/uniclipboard-admin-web/server.js /usr/local/bin/uniclipboard-admin-web"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1`

Expected: FAIL because Dockerfile does not install Node.js or copy admin web assets.

- [ ] **Step 3: Write minimal implementation**

Modify Dockerfile:

```dockerfile
USER root
RUN apk add --no-cache nodejs
COPY deploy/synology/admin-web/ /opt/uniclipboard-admin-web/
RUN chmod +x /usr/local/bin/uniclipboard-server-entrypoint \
 && chmod +x /opt/uniclipboard-admin-web/server.js \
 && ln -s /opt/uniclipboard-admin-web/server.js /usr/local/bin/uniclipboard-admin-web \
 && chown -R uniclip:uniclip /usr/local/bin/uniclipboard-server-entrypoint /opt/uniclipboard-admin-web
```

- [ ] **Step 4: Run wrapper test**

Run: `powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1`

Expected: PASS with `Synology server wrapper checks passed.`

- [ ] **Step 5: Commit**

```bash
git add deploy/synology/Dockerfile scripts/test-synology-server-wrapper.ps1
git commit -m "infra: package Synology admin web assets"
```

### Task 4: End-To-End Verification And Release

**Files:**
- No production file changes expected.

**Interfaces:**
- Consumes: committed implementation from Tasks 1-3.
- Produces: pushed commit and Docker Hub image.

- [ ] **Step 1: Run local verification**

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/test-synology-server-wrapper.ps1
node --check deploy/synology/admin-web/server.js
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 2: Inspect final diff**

Run:

```bash
git status --short
git log --oneline -5
```

Expected: only intended commits exist and no unstaged changes remain.

- [ ] **Step 3: Push**

Run:

```bash
git push origin main
```

Expected: remote `main` includes the implementation commits.

- [ ] **Step 4: Trigger Docker Hub workflow**

Use the existing GitHub workflow `.github/workflows/mirror-server-image-dockerhub.yml`.

Expected: workflow finishes successfully and publishes `chuais/uniclipboard-server:latest`.

- [ ] **Step 5: Report Synology parameters**

Tell the user to configure:

```text
UC_ADMIN_WEB=1
UC_ADMIN_PORT=42888
UC_ADMIN_PASSWORD=自定义管理密码
UC_MOBILE_PUBLIC_URL=https://your-domain.example:20221
```

And map:

```text
42888 -> 42888
```
