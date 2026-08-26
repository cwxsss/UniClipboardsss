# Project Memory and References

Use this document as the map for project memory sources. Read only what matches the task.

## Source Priority

When sources disagree, use this order:

1. Current code
2. `VISION.md` — 产品方向与底线（长期稳定，极少改动）
3. Root `AGENTS.md` navigation rules
4. Focused docs referenced by `AGENTS.md`
5. `docs/` current-state guides
6. `.planning/` roadmap and spike notes
7. DeepWiki / external references

## Project Memory Files

### Planning state

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.planning/STATE.md`

Read only when the task is about roadmap, planning, or requirement alignment.

## Existing Documentation Map

### High-level project docs

- `docs/README.md` — doc index
- `docs/overview.md` — product/system overview
- `README.md` / `README_ZH.md` — public project introduction
- `docs/development/config.md` — current default data/log paths across macOS, Linux, and Windows

### Architecture docs

Read these before structural work:

- `docs/architecture/principles.md`
- `docs/architecture/module-boundaries.md`
- `docs/architecture/bootstrap.md`
- `docs/guides/error-handling.md`

### Area-specific local guides

- `src/AGENTS.md` — frontend-local map
- `crates/AGENTS.md` — Rust workspace knowledge base (crates/ + apps/ + src-tauri/)

## External Reference

### DeepWiki

Project architecture reference:

- URL: `https://deepwiki.com/UniClipboard/UniClipboard`
- Access: use the DeepWiki MCP server when the task needs diagrams, historical architecture context, or flow explanations that are not obvious from code.

Do not treat DeepWiki as a higher authority than the repository code.

## 2026-08-26 工作阶段与构建诊断记录

- 当前阶段已明确为“线上内测与功能快速迭代”。后续排期优先保障系统工作流程、基础功能、算法效果、运行链路、ToB/C 端 API 以及线上内测部署。
- 深度测试、安全扫描、网络安全专项和全面生产加固暂列为后续阶段事项；当前仅记录必要风险，不主动改变功能开发主线。建议触发条件是：核心流程稳定、线上内测数据足以支持回归，或用户明确要求进入下一阶段。
- 鸿蒙端构建环境已验证：Engine 原生库在 DevEco 工具链下成功编译，使用 ASCII 工程副本规避 Windows `ld.lld` 无法读取中文真实路径的问题；新 HAR 已由 Hvigor 成功生成，Harmony 工程的 Engine 产物校验也已通过。
- 新 Harmony HAP 已成功完成 ArkTS 编译和打包，但项目未配置 `signingConfigs`。使用 DevEco 默认 `OpenHarmony.p12` 签名后，真机安装返回 `9568257: fail to verify pkcs7 file`；昨天调试助手生成的 `trial-app-signing.p12` 需要正确密码才能继续真机签名。设备在构建完成后再次锁屏，启动测试还需重新解锁。
- 鸿蒙端多空间加入逻辑的现有成员设备名兼容修复仍属于未提交工作树内容，后续提交前需要重新确认真实工程路径、构建入口和实机验证结果。
- 已建立全局五人持久子代理团队，职责覆盖跨项目架构、后端数据、前端交互、OCR/算法以及质量发布；统一使用 `gpt-5.6-luna`、`high` 推理。当前对话按需求选择最少必要成员，避免无关协作。

## 2026-08-26 多空间加入失败诊断与修复记录

- 桌面端 `POST /v2/spaces/join` 返回 `503` 的直接根因已由日志确认：第二个空间启动 Engine 时触发 `an iroh node is already running in this process`，不是邀请码、口令或设备网络问题。
- 根因是桌面与鸿蒙的多空间 supervisor 已按“每个空间一个 Engine runtime”实现，但 Engine 的 `uc-infra` 仍保留单进程节点租约，二者架构契约不一致。
- Engine 提交 `0ec02ed` 将单节点布尔租约改为可并行节点的计数租约；所有存活节点必须使用一致的 LAN-only 策略，最后一个节点关闭后才清理进程级策略。Engine 定向测试 `2 passed`，桌面 `space_runtime_supervisor` 定向测试 `22 passed`。
- 桌面 `Cargo.toml` 与 `Cargo.lock` 已固定到 `cwxsss/Engine` 提交 `0ec02ed`，桌面修复提交为 `c8bf34726`。提交钩子因环境缺少 `bun` 未能运行，提交使用已通过的定向测试结果完成。
- 鸿蒙端 ASCII 测试工程已临时替换为包含 `0ec02ed` 的 HAR 并成功生成 HAP；正式工程的版本目录和实机安装验证仍待真机签名完成后同步确认，不能提前宣称双空间实机验证完成。
- 深度安全扫描、全量跨设备压力测试和完整发布验证继续按项目阶段要求后置；风险是多空间在新 HAP 前仍可能复现旧单节点失败，触发条件是准备交付新的多空间测试包或线上内测扩大范围。
