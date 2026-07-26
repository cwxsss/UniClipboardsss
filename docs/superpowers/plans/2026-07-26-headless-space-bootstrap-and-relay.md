# Headless Space 置备与 Relay 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**目标：** 让群晖 headless 节点能通过环境变量明确创建新 Space 或无交互加入既有 Space，并提供独立的 iroh relay 部署目录。

**架构：** 群晖镜像仍是一个持久在线的 Space 成员和移动端网关，不承担 relay 职责。入口脚本以 `UC_SPACE_BOOTSTRAP_MODE` 作为唯一置备状态来源：`init` 调用 `uniclip init`，`join` 调用 `uniclip join`；只有本地 `/data` 尚未完成置备时才执行。iroh relay 位于独立 `deploy/relay/` 目录，拥有自己的 Compose、配置和证书数据卷，不读取任何 `UC_*` Space 凭据。

**技术栈：** POSIX shell、PowerShell、Docker Compose、Node.js、iroh relay Docker 镜像。

## Global Constraints

- 不保留旧自动初始化变量；`UC_SPACE_BOOTSTRAP_MODE` 是唯一的首次置备选择。
- `init` 必须提供 `UC_SPACE_PASSPHRASE`；`join` 必须同时提供 `UC_SPACE_INVITE_CODE` 和 `UC_SPACE_PASSPHRASE`。
- 已完成置备的 `/data` 重启时不得重新创建或重新加入 Space。
- Space 节点与 relay 不共享 Compose、卷、端口或凭据。
- 文档使用中文；代码注释使用英文；不得在文档、测试或示例中写入真实凭据。
- 新增持久化业务数据必须遵守 MasterKey AEAD 加密；本改动不新增业务持久化数据。

---

### Task 1: 为 headless bootstrap 建立回归测试

**Files:**
- Create: `scripts/test-synology-server-entrypoint.ps1`
- Modify: `scripts/test-synology-server-wrapper.ps1`

**Interfaces:**
- Consumes: `deploy/synology/uniclipboard-server-entrypoint.sh`
- Produces: 一个以伪 `uniclip` 可执行文件记录参数的测试夹具。

- [ ] **Step 1: 写入失败测试**

覆盖以下情形：`init` 调用 `uniclip init`；`join` 调用 `uniclip join --code ... --passphrase ... --device-name ...`；未知模式和缺失变量失败；已有 `.setup_status` 时不调用 `init` 或 `join`。

- [ ] **Step 2: 运行测试并确认当前入口不满足新契约**

运行：`pwsh -File scripts/test-synology-server-entrypoint.ps1`

预期：失败，因为当前入口没有 `join` 模式。

- [ ] **Step 3: 实现最小入口改动**

在入口脚本集中解析 `UC_SPACE_BOOTSTRAP_MODE`，通过一个 `bootstrap_space` 函数执行严格校验和对应 CLI 调用。

- [ ] **Step 4: 运行脚本回归测试**

运行：`pwsh -File scripts/test-synology-server-entrypoint.ps1`

预期：退出码为 `0`，所有 bootstrap 场景通过。

### Task 2: 更新群晖配置契约和部署说明

**Files:**
- Modify: `deploy/synology/uniclipboard-server.env.example`
- Modify: `deploy/synology/README_ZH.md`

**Interfaces:**
- Consumes: `UC_SPACE_BOOTSTRAP_MODE`、`UC_SPACE_INVITE_CODE`、`UC_SPACE_PASSPHRASE`、`UC_DEVICE_NAME`
- Produces: 可直接复制的创建和加入两套环境变量配置。

- [ ] **Step 1: 更新配置示例**

把默认示例改为 `UC_SPACE_BOOTSTRAP_MODE=init`，并用注释给出 `join` 所需的两项凭据。

- [ ] **Step 2: 更新群晖文档**

明确 headless 节点是 Space 成员而非 relay，分别说明创建新 Space 和加入既有 Space，说明加入邀请码只在首次启动时使用且不得提交到仓库。

- [ ] **Step 3: 运行已有管理页面测试**

运行：`pwsh -File scripts/test-synology-server-wrapper.ps1`

预期：退出码为 `0`，管理页面行为未回归。

### Task 3: 增加独立 iroh relay 部署工件

**Files:**
- Create: `deploy/relay/docker-compose.yml`
- Create: `deploy/relay/config.toml.example`
- Create: `deploy/relay/README_ZH.md`

**Interfaces:**
- Consumes: `IROH_RELAY_IMAGE`、`IROH_RELAY_DOMAIN`、`IROH_RELAY_CONFIG`
- Produces: 独立运行的 relay 服务，暴露 `80/tcp`、`443/tcp` 和 `7842/udp`。

- [ ] **Step 1: 创建 Compose 和最小配置示例**

使用固定版本的 `n0computer/iroh-relay` 镜像，挂载只读配置与证书/状态卷；不定义任何 Space 相关环境变量。

- [ ] **Step 2: 编写部署文档**

描述 DNS、80/443 TCP 与 7842 UDP 的前置条件、配置域名、启动验证、如何在每个支持 custom relay 的客户端填入 URL，以及 relay 不会解决不支持 relay 配置的旧客户端问题。

- [ ] **Step 3: 校验 YAML 与示例配置**

运行：`docker compose -f deploy/relay/docker-compose.yml config`

预期：Compose 语法通过且输出中没有 `UC_SPACE_` 或其他 Space 凭据。

### Task 4: 整体验证与变更检查

**Files:**
- Verify: `deploy/synology/uniclipboard-server-entrypoint.sh`
- Verify: `deploy/synology/README_ZH.md`
- Verify: `deploy/relay/`

**Interfaces:**
- Consumes: 前三个任务的实现和测试。
- Produces: 经验证且没有敏感信息的部署改动。

- [ ] **Step 1: 运行所有部署相关测试**

运行：

```powershell
pwsh -File scripts/test-synology-server-entrypoint.ps1
pwsh -File scripts/test-synology-server-wrapper.ps1
docker compose -f deploy/relay/docker-compose.yml config
```

- [ ] **Step 2: 扫描泄漏与废弃变量**

运行：

```powershell
rg -n "real-domain-placeholder|real-secret-placeholder" deploy docs/superpowers/plans
```

预期：部署实现与示例没有真实环境信息。

- [ ] **Step 3: 检查最终差异**

运行：`git diff --check; git status --short`

预期：无空白错误，变更仅限 bootstrap、部署文档、relay 工件与对应测试。
