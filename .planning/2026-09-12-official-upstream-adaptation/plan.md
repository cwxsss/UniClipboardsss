# 官方 Engine、PC 与鸿蒙能力适配计划

> **面向 AI 代理的工作者：** 本计划已获用户授权执行。改动按独立可验证阶段推进；提交、推送、Release 和 Docker 发布不包含在本计划授权内，除非用户另行明确授权。

**目标：** 将官方 UniClipboard 主线在 Engine、PC/daemon/Tauri 和 HarmonyOS 客户端中已经验证的网络、存储、启动、设备管理、托盘和桌面体验能力，适配到当前保留 Windows 多空间能力的代码体系；不能安全迁移的能力单独记录并保持可回滚。

**架构：** Engine 是空间、配对、加密、存储、搜索、成员恢复和跨端公共契约的唯一事实来源；PC 只负责桌面宿主、daemon、Tauri 和 UI 适配；鸿蒙只通过固定版本 Engine HAR/N-API 消费公共能力。当前 Windows 多空间运行时保留为宿主层能力，不从官方主线直接删除，连接恢复和启动流程通过明确的边界接入 Engine。

**技术栈：** Rust/Cargo workspace、uc-engine、Tauri 2、React 19/TypeScript/Vitest、ArkTS/ArkUI、HarmonyOS HAR/N-API、OpenAPI 生成客户端。

## 变更文件总览

### Engine 工作区 `D:/下载/codedit/UniClipboardsss/.engine-work`

- 修改：`Cargo.toml`、`Cargo.lock`，统一官方 Engine 版本和依赖。
- 修改：`crates/uc-engine/`，公开启动进度、连接机会、诊断和跨端操作契约。
- 修改：`crates/uc-application/`，接入连接恢复、成员恢复、启动升级、传输终态和搜索协调。
- 修改：`crates/uc-core/`，接入成员分支、设备组冲突和空间代际状态。
- 修改：`crates/uc-infra/`，接入 v3 profile 存储、密钥保险库、索引批量重建、Windows 升级恢复和网络诊断。
- 修改：`bindings/uc-engine-uniffi/`、`bindings/uc-ohos-napi/`，同步 iOS/Android/HarmonyOS 公共绑定，重点完成鸿蒙接口。
- 修改：`tests/`、`tests/hosts/ohos/`，补齐公共契约、迁移、连接恢复、诊断和鸿蒙宿主验收。
- 修改：`docs/`、`scripts/release/`，记录版本、产物来源、校验和迁移边界。

### PC 主仓库 `D:/下载/codedit/UniClipboardsss`

- 修改：根 `Cargo.toml`、`Cargo.lock`，只保留一个固定 Engine revision。
- 修改：`crates/uc-bootstrap/`、`crates/uc-desktop/`、`crates/uc-platform/`，承接宿主能力、系统唤醒、启动进度、诊断和视觉能力。
- 修改：`apps/daemon/`、`crates/uc-daemon-contract/`、`crates/uc-daemon-client/`、`crates/uc-webserver/`，扩展 daemon 与 Web API 契约。
- 修改：`src-tauri/crates/uc-tauri/`，接入 Tauri 命令、窗口、托盘、系统唤醒和视觉效果。
- 修改：`src/api/`、`src/components/`、`src/contexts/`、`src/hooks/`、`src/lib/`、`src/store/`、`src/pages/`、`src/i18n/`，同步官方 PC 功能和状态流。
- 测试：对应 Rust 契约测试、Vitest、Tauri specta 导出测试和 Windows 双端 E2E。
- 保留：`apps/daemon/src/daemon/production_spaces.rs`、`space_catalog.rs`、`space_runtime_supervisor.rs`、`windows_space_authority.rs` 及相关多空间 API；除非新的 Engine 集成已经替代且通过 Windows 回归，否则不删除。

### HarmonyOS 仓库 `D:/下载/codedit/UniClipboardHarmonyOS`

- 修改：`common/oh-package.json5`、`third_party/uniclipboard-engine/`，替换为同一个官方 Engine Release 的 HAR、N-API 声明、原生库和校验清单。
- 修改：`common/src/main/ets/engine/`、`common/src/main/ets/service/EngineRuntimeService.ets`，适配新增连接机会、诊断、启动升级、设备组、同步偏好和文件/媒体状态。
- 修改：`features/clipboard/`，把公共 Engine 能力接入共享 Controller，避免 Compact/Expanded 两套业务逻辑。
- 修改：`products/default/`、资源文件和测试，补齐启动恢复、设备管理、诊断和响应式 UI。
- 测试：`tools/verify-engine-release.ps1`、`devecocli build`、真机/模拟器后台同步和媒体接收验收。

## 阶段 0：基线、分支和公共版本锁定

- [ ] 在 Engine 仓库建立 `codex/official-rc15-adaptation`，保留当前 Engine 分支和 `.ohos-build` 状态。
- [ ] 在 PC 主仓库建立当前功能分支的实现基线，确认 `origin/main`、`upstream/main` 和 Engine revision 可追溯。
- [ ] 记录 PC、Engine、鸿蒙三仓库当前 commit、分支、远程、工作区未跟踪内容和工具链版本。
- [ ] 建立 `origin/main` 与 Engine `rc8` 的现有测试基线；把环境性失败与本次回归分开记录。
- [ ] 验收：三仓库状态清单完整；没有在 `main` 上实施；计划中的每个公共版本均能定位到 40 位 commit 或 Release。

## 阶段 1：Engine 公共契约和官方核心迁移

### 1.1 先迁移可被三端共同消费的接口

- [ ] 将官方 `rc15` 中的 `ConnectivityOpportunity`、`notify_connectivity_opportunity`、`StartupProgress`、诊断契约和终态事件纳入集成分支。
- [ ] 保留当前鸿蒙已有的 Engine host 能力，并把新增 host 回调设计为可选、语义化的能力，不把 ArkTS 或桌面 API 泄漏进 Engine。
- [ ] 同步 `uc-engine` 公共 binding 和 `uc-observability-contract` 的归属，避免 PC 和鸿蒙分别维护两份协议定义。
- [ ] 验收：`cargo check --workspace --locked`、`cargo test --workspace --locked`、`cargo fmt --all -- --check` 通过，或记录明确的工具链阻塞。

### 1.2 网络连接和成员恢复

- [ ] 迁移自动恢复配对设备连接、前台/系统唤醒/网络变化连接机会和地址恢复逻辑。
- [ ] 迁移成员历史反熵、设备组查询单快照、稳定冲突解释、选中设备组恢复和 durable membership recovery。
- [ ] 迁移配对取消并发重试、六位邀请代码和离线确认期间的移除行为。
- [ ] 为连接恢复、成员分支、离线恢复和冲突决策保留或补齐 Engine 单测、虚拟网络测试和 E2E。
- [ ] 验收：官方自动连接测试、成员拓扑测试、配对取消测试全部通过；没有新增旧/新双协议长期并存路径。

### 1.3 存储、搜索和性能

- [ ] 迁移 v3 profile storage、profile content key vault、空间代际目录和 v3 搜索保护组。
- [ ] 迁移旧 profile 升级、不可读旧载荷保留、Windows 崩溃恢复和升级 journal；任何迁移失败必须可观测且不能静默丢数据。
- [ ] 迁移搜索索引批量 staging 写入、重建与实时写入串行化、启动维护延后和 profile key 复用。
- [ ] 保持敏感字段默认密文、AAD 绑定、密钥 zeroize、日志脱敏和文件名边界；不因迁移性能而明文落库。
- [ ] 验收：profile upgrade、crash recovery、v3 search、key reuse、5000 条重建 benchmark 和现有数据回归通过。

### 1.4 传输、诊断和启动进度

- [ ] 迁移无归属终态收敛、传输取消和接收状态的公共事件。
- [ ] 迁移隐私安全本地诊断捕获、事件源注册、诊断导出和 host diagnostic receipt。
- [ ] 迁移 Engine 启动存储升级进度快照和重试通道，供 PC 与鸿蒙各自呈现。
- [ ] 验收：公共绑定契约、诊断导出、启动进度和传输状态测试通过；诊断包不包含剪贴板正文、密钥、口令和原始敏感路径。

## 阶段 2：PC daemon、宿主和 API 适配

### 2.1 宿主边界

- [ ] 在 `uc-platform` 增加系统唤醒/网络变化的稳定平台语义，Windows、macOS、Linux 分文件实现，unsupported/unavailable 明确返回。
- [ ] 在 `uc-bootstrap` 将宿主能力和 Engine runtime 装配连接起来，不把业务规则塞进平台层或 Tauri command。
- [ ] 在 `uc-desktop` 增加启动进度、离线恢复、诊断包导出、系统唤醒通知和视觉能力探测。
- [ ] 保留 Windows 多空间 authority/supervisor，明确它只拥有空间选择与宿主生命周期，Engine 拥有连接和成员真相。
- [ ] 验收：`cargo tree -p uc-desktop -e normal` 不包含 Tauri；平台契约测试和 Windows 多空间生命周期测试通过。

### 2.2 daemon/API/客户端

- [ ] 在 `uc-daemon-contract` 增加诊断 DTO、启动进度 DTO、连接机会请求、设备组呈现和必要的同步控制契约。
- [ ] 在 `uc-webserver` 增加诊断捕获/停止/导出、启动状态、连接机会和设备同步路由；保持 OpenAPI 为单一事实来源。
- [ ] 在 `uc-daemon-client` 生成并封装对应客户端；不在页面中直接调用 Tauri 或手写 HTTP。
- [ ] 更新 daemon 状态推送和错误映射，区分 unavailable、rebuilding、recovery-required、permission-denied 和 internal。
- [ ] 验收：OpenAPI 生成、契约测试、客户端测试、daemon 单测和诊断导出测试通过。

## 阶段 3：PC UI、窗口和托盘

### 3.1 启动、恢复和诊断

- [ ] 将启动页面拆为启动状态、升级进度、失败恢复和已认证内容，修复启动时设备信任对话框闪烁。
- [ ] 接入 Engine/daemon 启动升级进度、旧 profile 恢复、离线恢复操作和诊断导出。
- [ ] 使用统一状态源，避免 App、页面和组件分别维护启动阶段。
- [ ] 验收：Vitest 覆盖冷启动、升级中、失败可恢复、离线和重试；Windows 启动实际回归无黑窗和闪烁。

### 3.2 设备管理和托盘

- [ ] 移植设备列表刷新、添加设备后的成员刷新、移动设备活动状态和在线状态分离。
- [ ] 移植全局同步和按设备同步控制，保留现有同步偏好数据模型，必要时通过统一 daemon API 适配。
- [ ] 移植明确设备组选择、设备详情空间重建入口，并与 Windows 多空间 UI 做冲突分析。
- [ ] 验收：设备列表、设备详情、托盘菜单、跨窗口设置同步和设备离线/恢复测试通过。

### 3.3 窗口、历史和视觉效果

- [ ] 移植主窗口字体、CJK 字体、最小尺寸、标题栏拖动、对话框拖动、窗口控制按钮和 Linux 不透明表面修复。
- [ ] 移植历史虚拟列表宽度约束、覆盖滚动条、代码预览/行号背景和内联换行标记。
- [ ] 移植快速面板预览动作收敛、可展开预览操作、统一菜单/Toast/弹窗动画。
- [ ] 分阶段接入 Smooth Mode 和视觉能力探测；Linux 禁用不适合的重效果，Windows/macOS 使用完整效果。
- [ ] 验收：Vitest、Tauri 窗口测试、视觉效果 E2E 和 Windows/Linux 实机截图回归通过。

## 阶段 4：HarmonyOS 适配

### 4.1 固定 Engine Release

- [ ] 从 Engine 集成分支构建或取得一个完整、可追溯的官方 Engine Release，统一 HAR、`libuc_ohos_napi.so`、`index.d.ts`、版本文件、源码 commit 和 SHA-256。
- [ ] 更新 `common/oh-package.json5` 和 `third_party/uniclipboard-engine/<version>/`，不混用 rc8 HAR 与 rc15 声明或动态库。
- [ ] 运行 `tools/verify-engine-release.ps1`；校验版本、架构、最低 API、导出符号和产物清单。
- [ ] 验收：Engine 版本服务、N-API 契约测试和 `devecocli build` 先通过，再改业务 UI。

### 4.2 公共服务和业务 Controller

- [ ] 在 `EngineRuntimeService.ets` 增加连接机会、诊断、启动进度、设备组、同步偏好、传输终态和恢复操作的稳定包装。
- [ ] 在 host adapter 中实现系统唤醒/前后台、日志目录、文件句柄、安全资产和受管缓存能力；不把 Engine 原始 N-API 对象泄漏到产品层。
- [ ] 在 `ClipboardFeatureController.ets` 统一接入新事件和状态，Compact/Expanded 视图共用一个 Controller。
- [ ] 保持文本自动同步由 Engine 负责，图片/文件继续遵守用户指定目标设备和数据传输后台持续任务约束。
- [ ] 验收：后台接收文本、图片和文件、设备同步偏好、诊断导出、启动恢复、前后台暂停/恢复全部通过。

### 4.3 鸿蒙设备管理和界面

- [ ] 增加设备刷新、设备组状态、按设备同步控制和冲突/恢复提示。
- [ ] 增加诊断页面、启动升级进度和离线恢复入口；所有文案同步 `base` 与 `en_US` 资源。
- [ ] 适配手机、平板和二合一的 Compact/Expanded 布局，浅色/深色和中英文同时回归。
- [ ] 验收：模拟器与真机 `devecocli build`，使用同一 Engine Release 验证设备身份保持、文本后台同步、媒体接收和诊断包脱敏。

## 阶段 5：跨端回归、性能和交付门禁

- [ ] 运行 Engine workspace 全量测试和公共绑定契约测试。
- [ ] 运行 PC `cargo check --workspace`、`cargo test --workspace`、`bun run build`、`bun run test`、`bun run lint`、Tauri specta 导出测试。
- [ ] 运行 Windows 双 profile、多空间切换、设备离线/恢复、配对取消、启动升级和历史搜索回归。
- [ ] 运行 HarmonyOS 构建、后台同步、媒体接收、设备管理和诊断导出回归。
- [ ] 生成版本/commit/HAR/原生库/校验值/测试设备清单，确保 PC 和鸿蒙消费同一 Engine Release。
- [ ] 只在所有阶段有证据后，再讨论 commit、推送、Release 或镜像发布；本计划不自动执行这些外部动作。

## 暂缓但保留为明确阶段

以下内容纳入目标，但在前置 Engine 迁移和跨端契约稳定前不与 PC UI 混合实施：

1. v3 profile storage。
2. profile content key vault。
3. 搜索索引批量重建。
4. 成员历史反熵。
5. 多分支成员冲突恢复。
6. 旧 profile 升级和崩溃恢复。
7. 完整配对协议迁移。

它们不是放弃，而是以 Engine 为中心的独立迁移门。任何一个阶段无法完成时，必须保留当前 rc8/现有多空间路径，不能长期并存两套未定义权威的实现。

## 全局验收原则

- Engine、PC、鸿蒙必须使用同一固定版本的公共契约和发布产物。
- 不复制 Engine 内部 Rust 包到 PC 或鸿蒙，不在产品仓库维护 Engine 核心补丁副本。
- 不删除当前 Windows 多空间能力，除非新的 Engine 空间运行时已经完成等价替代并通过实机回归。
- 所有持久化业务负载默认加密；诊断、日志、错误和测试数据不得泄露敏感用户内容。
- 所有跨层变更必须有对应契约测试；静态检查不能替代 Windows、鸿蒙真实运行路径验证。
