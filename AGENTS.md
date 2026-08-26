# AGENTS.md

This file is the root navigation index for repository instructions.

## Core Rule

Do not treat this file as a full memory dump. Read only the documents needed for the current task.

## Always Apply

- **持久化默认密文（不可打破的基石）**：任何写入持久化存储的业务负载——SQLite 数据库列、磁盘缓存、搜索索引——默认必须先经 MasterKey AEAD 加密，**严禁明文落库**（剪贴板正文、标题、预览、搜索渲染字段、标签名、文件名和文件路径等一切用户内容）。例外包括内容类型分类枚举（text/image/file/link 等）、文件内容本体，以及入站文件在受管文件缓存中的经安全清理的原始文件名；该文件名只能作为实际缓存文件的 basename，原始目录路径、数据库/搜索字段、日志及关联元数据仍须加密或脱敏。新增其他持久化列/文件默认加密，主张明文需在 PR 中论证并获批。详见 `VISION.md`「安全与隐私底线」与「绝对禁区」。
- Fix root causes, not symptoms.
- Use the standard, established terminology of software engineering and computer science. If a metaphor or coined expression is unavoidable, also give the corresponding standard term; never invent new words for the sake of novelty.
- Preserve single source of truth and clear ownership.
- Do not keep parallel old/new logic without a removal plan.
- Use repo-relative paths in tracked docs.
- Use language identifiers on fenced code blocks.
- When conversation is in Chinese, respond in natural Chinese.
  - Do not mix Chinese and English within a Chinese reply, except for established proper nouns / technical terms (e.g. crate names, API identifiers, file paths) that have no natural Chinese equivalent.
- **项目文档**（`docs/`、README、crate 级 `AGENTS.md`、`CONTRIBUTING*.md`）使用中文。此约定覆盖全局 `CLAUDE.md` 中"写文档用英文"的默认规则。
  - 引用外部规范（RFC、标准库 API 等）时，专有名词保留英文原文。
- **代码注释**（`//` / `///` / `/* */` / doc comments）使用英文。存量中文注释不强制迁移，新增或修改注释时按英文撰写；踏到旧中文注释可顺手改成英文。
  - 代码标识符（函数、类型、变量名）、Git commit message、PR 标题与描述同样保持英文。
- **不强制语言审查的开发自留路径**：`.planning/`（调研/spike 笔记）、`.agents/`（项目技能）、`.codex/`（Codex 配置与工具）、`publish = false` 的诊断 crate（例如 `crates/p2p-bench`）。这些目录按写作者方便即可，CodeRabbit 也已在 `.coderabbit.yaml` 中跳过。
- `CLAUDE.md` is only a compatibility entrypoint. This file is the root instruction source.

## 项目阶段与工作优先级

- 当前阶段：线上内测与功能快速迭代。
- 核心职责：快速开发整套系统工作流程，打通基础功能，完善算法效果与运行链路，完善 ToB/C 端 API 接口，并将这些能力部署到线上内测环境。
- 排期原则：所有工作优先服务上述职责。不得主动把主要工作重心转向网络安全专项、全面生产加固或复杂测试工程。
- 延后事项：认为必要但当前应降级的深度测试、安全扫描、网络安全和生产加固工作，必须记录到 `docs/agent/project-memory.md`，写明原因、风险和建议触发条件；除非用户明确要求，不因这些事项改变当前开发优先级。
- 阶段升级提醒：结合项目成熟度判断，当上述被延后的内容应进入下一阶段时，在对话中明确提醒：`项目应该进入下一阶段，应该更新 agents 了`。
- 交付记录：每次完成对话中的工程工作后，若产生了新的事实、决策、验证结果或风险，更新 `docs/agent/project-memory.md`；避免重复记录无新增信息的内容。
- 团队启用：每个对话根据需求自动判断是否启用全局团队成员。涉及跨项目统筹、桌面端、鸿蒙端、Engine、发布或验证时，优先启用对应职责的成员；范围明确的单项任务只启用必要成员，不为形式而扩大协作范围。

## Read-on-Demand Map

### 0. Product direction / architecture decisions

Read: `VISION.md`

Use when:
- making product decisions (add/remove features, change UX paradigm)
- evaluating whether a proposal violates project principles
- judging issue priority or scope
- changing architecture direction or locked decisions
- reviewing whether a PR aligns with project goals

**This is the first document to consult before any non-trivial decision.**

### 1. General code change / bug fix / review
Read: `docs/agent/workflow-rules.md`

Use when:
- fixing bugs
- evaluating whether a change is a patch or a refactor
- processing AI review comments
- updating docs or scripts with repository hygiene constraints

### 2. Architecture / boundaries / commit planning
Read: `docs/agent/architecture-rules.md`

Use when:
- changing crate boundaries
- adding ports/adapters
- touching cross-crate DTO conversions
- planning commit splits
- reviewing whether a diff mixes multiple intents

### 2a. Port definition / evolution / refactoring
Read: `docs/architecture/ports.md`

Port 的可写事实来源现位于 `UniClipboard/Engine`。本仓中的这份文档只作为迁移期参考；不得在 desktop 下重新创建或修改 Engine 内部包。

Use when:
- 规划必须在 `UniClipboard/Engine` 实现的 Port 变更
- adding methods to existing port traits
- deciding port granularity or naming
- refactoring large port interfaces into smaller ones
- reviewing whether a use case depends on more than it needs

### 3. Rust / Tauri / daemon / tracing work
Read: `docs/agent/rust-tauri-rules.md`

Use when:
- editing Rust code
- adding or changing Tauri commands
- handling async loops, network drivers, or daemon APIs
- working on tracing/logging
- emitting frontend events from Rust
- running cargo commands

### 4. React / TypeScript / Tailwind / UI work
Read: `docs/agent/frontend-ui-rules.md`

Use when:
- editing React or TypeScript UI code
- adjusting layouts or styling
- touching theme behavior
- working on frontend DTO handling or frontend tests

### 5. Project memory / historical lessons / deeper references
Read: `docs/agent/project-memory.md`

Then selectively read:
- `docs/README.md` and linked docs for current-state guidance
- `.planning/` for roadmap, milestones, and spike research notes
- `src/AGENTS.md` for frontend-local navigation
- `crates/AGENTS.md` for Rust-workspace navigation (crates/ + apps/ + src-tauri/)
- `src-tauri/AGENTS.md` for Tauri packaging specifics
- `apps/cli/AGENTS.md` for `uniclip` CLI-local rules

Log file locations (platform-conventional, separate from the data root; single
source of truth is `uc_app_paths::app_log_dir()`):
- macOS: `~/Library/Logs/app.uniclipboard.desktop[-<profile>]/`
- Linux: `~/.local/state/app.uniclipboard.desktop[-<profile>]/logs/`
- Windows: `%LOCALAPPDATA%\\app.uniclipboard.desktop[-<profile>]\\logs\\`

Per-role files (`uniclipboard-{gui,daemon,cli}.json.<date>`), daily rotation,
7-day retention (older files pruned on start). Portable builds keep logs under
`<exe>/data/logs/`.

Do not assume the older `uniclipboard` root is current, and note logs are no
longer under the data root's `logs/` subdir on macOS/Linux. The app dir name is
`app.uniclipboard.desktop`, with an optional `UC_PROFILE` suffix such as `-dev`.

Use when:
- entering an unfamiliar subsystem
- trying to understand why a pattern exists
- doing structural work that depends on past decisions

## Practical Loading Order

### Frontend task
1. `AGENTS.md`
2. `docs/agent/frontend-ui-rules.md`
3. `src/AGENTS.md`
4. relevant code/docs only

### Rust/Tauri task
1. `AGENTS.md`
2. `docs/agent/rust-tauri-rules.md`
3. `docs/agent/architecture-rules.md` if boundaries are involved
4. `crates/AGENTS.md` (plus `src-tauri/AGENTS.md` for packaging work)
5. relevant code/docs only

### Complex bug in unfamiliar area
1. `AGENTS.md`
2. `docs/agent/workflow-rules.md`
3. `docs/agent/project-memory.md`
4. selective reads from `.planning/`, local `AGENTS.md`, and targeted docs

## Files Managed by This Index

- `VISION.md` — 产品方向、架构原则、锁定决策、绝对禁区
- `docs/agent/workflow-rules.md`
- `docs/agent/architecture-rules.md`
- `docs/architecture/ports.md`
- `docs/agent/rust-tauri-rules.md`
- `docs/agent/frontend-ui-rules.md`
- `docs/agent/project-memory.md`

If new global guidance is added, prefer placing it in one of those focused documents and only add a pointer here.
