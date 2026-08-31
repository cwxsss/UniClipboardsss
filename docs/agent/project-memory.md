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

## 2026-08-27 鸿蒙模拟器 rc.6 构建与安装验证

- DevEco Studio 6.1 已创建并启动 API 24、HarmonyOS 6.1.1 的 `x86_64` 手机模拟器，HDC 目标为 `127.0.0.1:5555`。
- 首次未签名 HAP 安装失败的直接原因是 HAP 仅包含 `arm64-v8a` 原生库，而模拟器要求 `x86_64`；这与 Engine 协议版本无关。按当前 Engine 提交 `0ec02ed` 编译了 x86_64 Engine 和鸿蒙 native 桥接库，ARM 真机库保持不变。
- 多 ABI HAP 已由 Hvigor 成功打包，实际同时包含 `libs/arm64-v8a` 与 `libs/x86_64`。Release Profile 因缺少 `ohos.permission.READ_PASTEBOARD` 调试 ACL 被模拟器拒绝；改用 DevEco 本地 Debug Profile 并声明该 ACL 后签名成功。
- `com.sss.uniclipboard` 已通过 HDC 安装并启动，系统识别 `cpuAbi=x86_64`、原生库路径为 `libs/x86_64`，进程保持运行。当前 x86_64 库属于本地模拟器调试产物，不能直接替代 ARM 真机或正式 Release 资产。

## 2026-08-27 鸿蒙模拟器功能回归验证

- 在 `127.0.0.1:5555` 模拟器上验证了同步页、设备页、设置页、空间创建/加入页和连接诊断页的进入与返回，页面均正常渲染，应用进程未发生崩溃。
- 使用临时测试口令创建空间成功，生成空间 ID 并打开设备邀请；输入口令后连接二维码成功生成并显示。重启应用后空间仍保留，退到后台 15 秒后进程仍在，回到前台后状态正常。
- 多空间管理页的“加入其他空间”按钮可用，加入模式表单可显示；二维码扫描页可以打开相机预览并正常退出。同步页“粘贴”成功读取模拟器系统剪贴板文本，图片读取入口点击后应用保持稳定。
- Engine 日志持续出现 `operation_finished`；在当前没有第二台桌面设备的前提下，自动发送目标数为 0，属于预期状态。未发现应用进程对应的 `SIGSEGV`、`SIGABRT`、原生库 `dlopen` 失败或 Engine 操作失败；日志中的 `libhint2type.z.so` 缺失属于模拟器系统组件告警。
- 本轮尚未完成真实跨设备文本/图片/文件收发，也无法在没有第二台设备的模拟器中验证对端同步类型开关；下一步需要连接桌面端或 ARM 真机进行双端验证。

## 2026-08-27 桌面端与鸿蒙模拟器跨设备回归

- 使用已打开的桌面端 `v1.0.0-alpha.7`、当前运行的 Engine，以及 `Engine v1.1.0-rc.6` 的鸿蒙 `x86_64` 调试 HAP 完成联调；桌面端 GUI、daemon、Engine 和鸿蒙应用进程均保持运行。
- 鸿蒙创建的空间可以被桌面端通过 `POST /v2/spaces/join` 加入，接口返回 `200`；双空间运行期间设备在线，测试结束后已将桌面端默认发送空间恢复为原来的空间。
- 文本双向同步通过；鸿蒙退到后台后，桌面端发送的文本仍能在回到前台后读取，后台进程未退出。桌面 GUI 日志确认默认快捷面板快捷键已注册为 `alt+v`，设置接口返回启用状态。
- 桌面端发送图片到鸿蒙通过，Engine 日志显示直连传输 `accepted=1`，鸿蒙端显示图片预览、图片读取和发送控件；鸿蒙端发送图片回桌面也通过，桌面剪贴板由唯一文本哨兵恢复为 `640x260` 位图，日志确认入站 `image`、`receiver_apply` 和写回系统剪贴板。
- 桌面端发送二进制文件到鸿蒙通过，Engine 日志显示文件传输 `accepted=1`、`blob_ref_count=1`，鸿蒙端显示文件预览和保存入口。当前文本内容的 `.txt` 文件会被鸿蒙端先按文本分支处理，原因是 `consumeOfficialEngineClipboard` 在 `applyOfficialEngineFile` 之前调用了文本判定；该问题待确认后修复，不能与二进制文件成功混为一谈。
- 当前模拟器 OCR 对测试图片返回“未识别到文字、链接、电话号码或二维码”；图片传输和预览本身正常，暂按模拟器 OCR 模型或能力限制处理，真实设备 OCR 仍需单独验证。
- 测试期间未发现鸿蒙应用崩溃、`SIGSEGV`、`SIGABRT`、Engine 操作失败或原生库加载失败。桌面日志中的 `CF_BITMAP`/`CF_DIB` 读取告警和网络地址探测告警未阻断本轮传输，暂不作为功能失败依据。
- 后续验证重点：确认 `.txt` 文件应以文件卡片展示并可保存；在 ARM 真机复测图片、文件和 OCR；继续观察桌面端快捷面板实际按键唤起和窗口交互。除非用户确认，不在本轮直接改动上述未确认行为。

## 2026-08-27 鸿蒙 HAP 内测包交付

- 当前鸿蒙工程使用 `hvigorw assembleHap --mode module -p product=default -p buildMode=debug --no-daemon` 构建成功，构建前置校验确认 Engine 为 `v1.1.0-rc.6`、源码提交 `0ec02ed96a0ee0afe6d72552104ced75a956c270`，后台同步模式校验通过。
- 命令行工程配置未直接声明签名配置，原始产物是未签名 HAP；随后使用本机已有的内测签名材料完成本地签名，`hap-sign-tool verify-app` 校验通过。最终包同时包含 ARM 和 `x86_64` 原生库，可用于当前内测设备安装。
- 本次仅重新生成鸿蒙 HAP，没有修改桌面端或 Engine 代码；桌面端现有安装无需重新安装。HAP 不包含尚未确认的 `.txt` 文件类型修复。

## 2026-08-27 鸿蒙中继设置保存修复与 rc.7 重验证

- 中继设置页面此前使用占位逻辑：读取页面会重置开关和地址，保存操作也会清空输入而没有调用 Engine，因此地址会表现为“保存后自动消失”。鸿蒙端现已通过 `EngineRuntimeService` 和 `ClipboardFeatureController` 调用 Engine 的查询、更新和中继探测接口，失败时保留用户输入。
- 真实日志进一步确认第二个根因：只修改自定义中继地址时，Engine 仍会启动空的凭据恢复事务；鸿蒙模拟器安全存储事务返回 `UC_ENGINE:1392:internal:false`，导致界面显示保存失败。`RelayCredentials` 现在仅在确有凭据增删改时写恢复事务，地址变更仍按正常设置路径持久化；凭据变更继续保持事务保护。
- Engine 修复提交为 `b79d754c2d5c53936500bb0b300ae81e9da403d0`，定向设置测试 `81 passed; 0 failed`，两种鸿蒙目标的 `uc-ohos-napi` 交叉编译均成功。未将 `.local-tools/` 机器专用文件加入提交。
- 重新打包过程中发现 Hvigor 首次命中了 ASCII 构建副本中旧的 `oh_modules` 缓存，虽校验通过但 HAP 哈希未变化；清理该临时副本的 `oh_modules`、`.hvigor` 和入口构建输出后重新安装本地 HAR，最终 HAP 已内嵌新 Engine 原生库。该缓存问题记录为构建流程风险，后续必须在 HAP 发布前核对包内库哈希，不能只看 Hvigor 的成功状态。
- DevEco 构建前置校验确认 Engine `v1.1.0-rc.7` 与上述提交一致，后台同步模式校验通过；`hap-sign-tool verify-app` 校验通过。最终调试 HAP 位于 `D:\下载\codedit\UniClipboardHarmonyOS\artifacts\sssUniClip-relay-settings-rc7-debug-20260827.hap`，SHA-256 为 `0A9149F252B29C6ABC2E78B848E8F82D40892AD8ADD74C86D86A6AA392E833E0`。
- 已在 `127.0.0.1:5555`、HarmonyOS 6.1.1、`x86_64` 模拟器上更新安装并启动 HAP。填写 `https://relay.chatsss.top` 后保存页面显示成功；强制停止并重启应用，再次进入页面仍能恢复该地址。保存后的 Engine 日志没有出现 `ENGINE_RUNTIME_ENGINE_UNAVAILABLE` 或 `1392/internal`，应用进程保持运行。
- 本轮仍未替代 ARM 真机验证，签名 HAP 同时包含 `arm64-v8a` 和 `x86_64`；真机中继可达性和实际跨设备中继链路需在下一轮内测确认。当前阶段继续后置全面安全扫描和生产加固。

## 2026-08-27 全量架构梳理与优化候选

- 当前产品的主链路是：桌面端 React/Tauri 作为界面客户端，通过 daemon 的本机 HTTP/WebSocket 访问 Engine；鸿蒙端 ArkUI 通过 `EngineRuntimeService` 调用官方 `@uniclipboard/engine` HAR；Engine 通过 `HostCapabilities` 接收宿主的系统剪贴板、文件、目录和安全存储能力。
- Engine 的职责不是单一网络库，而是稳定的运行时边界：`uc-core` 提供领域模型和 Port，`uc-application` 编排用例与 Facade，`uc-infra` 实现 SQLite、OpenMLS、iroh、blob 和中继能力，`uc-engine` 负责组装、生命周期、操作分发和事件流。空间、配对、成员信任、同步、历史、文件传输和恢复均从该边界进入。
- 当前三棵工作树的 Engine 版本没有完全对齐：桌面 `Cargo.toml`/`Cargo.lock` 固定在 `cwxsss/Engine` 的 `0ec02ed`、`v1.1.0-rc.6`；鸿蒙当前本地 HAR 和 `_engine_upstream` 工作树为 `v1.1.0-rc.7`、`b79d754`。本地 `_engine_upstream` 不会自动改变桌面 Cargo 解析结果。后续发布前必须选定唯一 Engine 版本和提交，并让桌面、鸿蒙 HAR、绑定声明和验证矩阵一致。
- 鸿蒙当前同时打包官方 Engine 原生库和 `rust/uniclipboard-native`。后者还编译 `rust/space-core`、`uc-application`、`uc-infra` 等另一套核心图，当前主要承载显式 LAN/HTTP 兼容功能，但仍保留旧空间节点、配对和发送接口。它造成构建、包体和维护成本，且存在状态分叉风险；在确认 LAN/HTTP 兼容需求后，应隔离或迁移兼容能力，再移除旧 P2P 入口。
- 当前桌面多空间的 `DesktopClipboardHub`、`ClipboardRouter`、`SpaceRuntimeSupervisor` 不是重复实现：它们分别保证一个物理剪贴板监听器、活动发送空间串行化和每空间运行时管理。桌面默认历史空间仍走兼容分支，其他空间走 supervisor；该双路径属于迁移复杂度，不能在未完成数据迁移前直接删除。
- 已确认的清理候选是鸿蒙当前产品视图中仅剩的 `SpaceNodeService`/`NativeSpace*` 导入，以及根目录 `entry/` 兼容快照；前者可在编译验证后清理，后者需先确认 CI、脚本和文档无引用。`setSpaceBackgroundMode` 仍调用旧 native 全局状态，而当前产品主运行时由官方 Engine 管理，需动态确认其是否仍有作用后再移除。
- 性能候选包括：桌面空间列表逐个重新读取目录并查询 Engine，可改为带修订号的内存目录和有界并发摘要；桌面加入空间完成依赖 200ms 轮询信任状态，可改为 Engine 事件等待并保留超时；鸿蒙剪贴板后台目前 200ms 轮询且自动捕获路径只组装文本，可评估系统观察器和自适应退避；Engine 文件发送当前先复制到 `engine-imports` 再进入 blob 流程，可基准确认是否存在可避免的双重 I/O。
- 暂缓事项：Engine `Operation` 公共枚举过宽、鸿蒙 NAPI 通过本地接口强制类型转换、默认空间与多空间运行时统一、旧兼容栈最终下线。这些属于结构性调整，先完成版本统一和实机回归，再分批处理。全面安全扫描、生产加固和复杂压力测试仍按当前线上内测阶段后置。

## 2026-08-27 桌面多空间接收状态语义

- 桌面顶部空间卡片由 `SpaceSelector` 展示，点击卡片实际调用 `PUT /v2/spaces/active-send`，作用是切换当前发送空间；卡片上的“接收状态”是只读汇总，不是直接开关。
- `incomingSyncState` 来自对应 Engine 的 `QueryReceiveReadiness`。当前 daemon 会把接收未就绪、查询失败和运行时不可用统一映射为“已停用”，因此界面无法区分全局同步关闭、空间会话尚未恢复和接收恢复失败。
- 启用第二个空间的现有路径是：先切换到该空间，再在本机设备的“全局同步策略”打开“启用同步”；选中具体已配对设备后，还要在设备详情的“同步设置”打开该设备的“接收”开关及需要的内容类型。若顶部仍为“已停用”，应查看 Engine 恢复状态和日志，而不是继续重复点击顶部卡片。
- 后续 UI 优化应把空间当前发送选择、空间 Engine 接收就绪、设备级接收策略拆成三个明确字段，并展示不可用原因；本轮仅记录语义，未修改代码。

## 2026-08-27 自动文字与手动媒体策略核对

- 当前目标是“文字复制后自动同步到其他设备；图片和文件复制后不自动发送，只有用户明确选择目标设备并点击发送时才传输”。现有 Engine 的自动本地捕获会把系统剪贴板快照交给统一 outbound planner；桌面端快照可以包含图片和文件，且默认文件同步开启，因此当前并未形成严格的“媒体仅手动”策略。
- `ClipboardChangeOrigin::LocalCapture` 同时承载系统自动捕获和鸿蒙 `SendImage`/`SendFiles` 手动入口；Engine 目前没有独立的手动媒体来源语义。`target_devices` 为空表示不设置目标过滤，发送到所有符合设备策略的成员。
- 鸿蒙后台自动捕获目前只提取 `text/plain`，但自动目标由选中的单个远端设备决定；目标为空时会跳过自动发送，因此不能代表“所有其他设备”。鸿蒙历史发送菜单把设备名称传入 `send*ToTarget`，控制器当前仅记录该参数，随后仍调用空目标列表，单设备选择没有真正生效。
- 要满足目标，需要拆分“自动文字”和“手动图片/文件”两条契约：自动捕获只允许文本并按空间内所有合格成员 fan-out；图片/文件必须由 UI 传入明确设备 ID 和手动来源，禁止空目标默认广播，同时保留全局同步及每设备收发策略作为最终门控。本轮仅完成代码核对，未修改实现。

## 2026-08-27 架构清理只读审计

- 本轮只读盘点未执行删除、移动、清空或覆盖。鸿蒙当前产品入口由 `products/default` 提供，根目录 `entry/` 是 39 个已跟踪文件组成的旧兼容快照；`tools/verify-background-sync-mode.ps1` 仍检查该快照，因此删除前必须先迁移校验脚本并完成构建验证。
- 鸿蒙当前使用的 Engine 组合是 `third_party/uniclipboard-engine/v1.1.0-rc.7`，来源提交为 `b79d754c2d5c53936500bb0b300ae81e9da403d0`，包含 HAR、ARM/x86_64 原生库、NAPI 声明和发布校验清单，属于现用交付物。桌面仍固定在 Engine `v1.1.0-rc.6` 提交 `0ec02ed`；版本统一必须以同一源提交重新生成并核对 HAR 与声明，不能只替换版本字符串。
- 鸿蒙 `uniclipboard_native` 仍被当前 `common`、剪贴板控制器和产品视图接线，实际承载 LAN/HTTP 兼容服务，同时还保留旧空间节点和发送接口；`rust/space-core` 与 `rust/uc-mobile` 仍在其原生构建图中。它们目前不能直接删除，需先确定鸿蒙是否下线 LAN/HTTP，再完成官方 Engine API、界面和运行时迁移。
- 桌面 `production_spaces.rs` 仍把 `profile_dir == "."` 的默认空间交给独立 `legacy_engine`，只有其他空间交给 `SpaceRuntimeSupervisor`；桌面顶部 `SpaceSelector` 仍负责发送空间选择。产品若隐藏顶部多空间 UI，仍需保留内部空间数据和 supervisor 管理，避免误删已加入空间或破坏后台接收。
- 只读统计显示主要可重建占用来自鸿蒙 `rust/space-core/target`、`rust/uniclipboard-native/target`、桌面 `t7a` 和 Engine 工作树 `target`；`node_modules`、鸿蒙 `oh_modules` 属于开发环境，当前 HAP、rc.7 HAR 和模拟器 x86_64 库属于交付/验证资产。清理这些目录前必须确认无构建进程，并按 A/B/C 清单取得精确路径批准。
- 性能优化暂不凭代码猜测：启动、空间切换、后台耗电和文件 I/O 先做桌面与 ARM 真机基准，再按数据修改；鸿蒙模拟器只能作为流程回归，不能替代耗电和真实传输测量。

## 2026-08-28 三端统一、最终构建与实机验证

- 当前运行契约统一为 Engine `v1.1.0-rc.7`。桌面 `Cargo.toml` 和 `Cargo.lock` 固定到 `ff493cfa8563cdd7fbf8615ed0a95b9058714176`；鸿蒙使用同一提交生成的固定 HAR，包含 `arm64-v8a` 与 `x86_64` 原生库及固定 NAPI 声明。Engine 用户分支随后追加的 `78b8788` 只补充 HAR 构建和契约测试，不改变运行时二进制，因此交付物仍以 HAR 的源提交 `ff493cf` 为准。
- 鸿蒙旧 `entry/` 快照、旧 `rust/` 原生核心、`uniclipboard_native` 构建链、旧 rc.3/rc.5/rc.6 Engine 包和无有效引用的旧 `tools/build-native.ps1` 已按根目录边界清理。当前产品入口为 `products/default`，官方 Engine HAR 为唯一运行核心；保留的网络设置仅属于 Engine 中继配置，不再保留旧 LAN/HTTP P2P 主链路。
- 当前策略实现为：系统自动捕获只自动发送文本和富文本；图片和文件保留在 Engine 历史中，必须由界面选择明确目标设备后显式发送，不再用空目标列表广播媒体。桌面空间运行时统一由 `SpaceRuntimeSupervisor` 管理，顶部旧多空间选择器已移除；空间目录、活动发送空间和设备级接收策略仍由 Engine/daemon 的正式接口维护。
- 最终验证结果：桌面前端 `1026 passed, 1 skipped`，daemon `121 passed`，Engine OHOS 工程契约 `19 passed`；Engine `uc-engine` 为 `121 passed, 2 failed`，两项失败均是 Windows 缺少 `bash` 导致既有明文扫描测试报 `program not found`，不是产品编译或同步逻辑失败。鸿蒙当前源码经 DevEco Hvigor 成功生成 HAP，`hap-sign-tool verify-app` 成功，带 `READ_PASTEBOARD` 调试 ACL；卸载旧包后重新安装并启动 `com.sss.uniclipboard`，模拟器识别 `x86_64` 原生库，应用进入同步页和设备页均无崩溃。
- 桌面最终构建通过，GUI 能拉起 daemon；隔离 profile 的 `/health`、`/auth/connect`、空间初始化和邀请生成接口均成功。同机第二桌面加入测试以及鸿蒙模拟器加入测试均因当前网络环境无法访问 `https://rendezvous.uniclipboard.app` 而未完成：日志为 TCP `os error 10013`，Engine 随后只铸造 LAN/mDNS 本地邀请，加入端返回 `not_found`。因此本轮未宣称真实跨设备文本、图片、文件收发全部通过；自动文本、显式媒体目标和多空间生命周期由 Engine/daemon/ArkTS 契约测试覆盖，真实双端链路需在可访问 rendezvous 或同一可发现局域网的设备上复测。
- 最终交付物：桌面便携程序与 NSIS 安装包位于仓库 `artifacts/pc/`，鸿蒙签名 HAP 位于 HarmonyOS 仓库 `artifacts/`。HAP 同时支持 ARM64 和当前 x86_64 模拟器，并非仅 ARM 单架构包；发布前必须保留包内库哈希和签名校验记录。
- 构建告警记录：Hvigor 可能对 HAR 自动生成的 `Index.d.ets` 报 `@ts-nocheck`，并提示 HAR 缺少 `sourceMapsPath`；当前源码无对应指令或运行时引用，且构建、签名和安装均成功，暂列为 DevEco 工具链告警。Cargo 的未使用变量、Vite 的大 chunk 和动态导入告警也未阻断当前内测交付。
- 本轮完成最终构建后可删除的临时缓存仅限已核对的桌面 `t7a`、`_toolshim`、桌面 `target`，Engine `target`、`.build-ohos-ninja`、`.build-ohos-win`、`.build-ohos`、`.local-tools` 以及 `C:\ucbuild` 下的临时构建副本；不得删除源码、锁文件、`vendor` 子模块、当前 rc.7 HAR、签名材料、最终 PC/HAP 交付物或运行数据。执行环境变量中的 `127.0.0.1:9` 只按进程清除，用户 Git 配置中的 `127.0.0.1:20808` 必须保留。
- 后置事项：补齐 Windows `bash` 扫描测试运行环境、在 ARM 真机复测中继和跨设备媒体收发、基于真实基准优化启动/空间切换/后台耗电/文件 I/O。当前项目仍处于线上内测与快速迭代阶段，除非核心流程稳定或用户明确要求，不把这些事项升级为生产安全专项。

## 2026-08-28 最终交付分支与资产记录

- 鸿蒙源代码已在用户仓库创建不含 LFS 历史的新分支 `codex/rc7-source`，提交为 `426ea9a`；公开 fork 拒绝上传超过单文件限制的 HAR，因此源码保留 rc.7 元数据、声明、校验清单和下载说明，`UniClipboardEngine.har` 作为 Release 资产分发，本地构建副本继续保留。
- Engine 用户分支 `codex/multispace-engine-integration` 已推送提交 `3c81e9e`；该提交补充 rc.7 OHOS 冒烟版本契约，桌面和鸿蒙运行时仍共同固定源提交 `ff493cfa8563cdd7fbf8615ed0a95b9058714176`。
- 桌面便携程序 `artifacts/pc/UniClipboard.exe` 的 SHA-256 为 `bb1da6ca96c81de91cd82468dd2c57e24031e12ffabe64f4e79c2b894c8eddcf`；NSIS 安装包 `artifacts/pc/UniClipboard_1.0.0-alpha.7_x64-setup.exe` 的 SHA-256 为 `1fe3efb47b87b7c0128e4535c2cb9d8e80134061b890faaf5fd345084a219d57`。
- 鸿蒙签名交付物 `D:/下载/codedit/UniClipboardHarmonyOS/artifacts/sssUniClip-rc7-debug-signed-acl-20260828.hap` 的 SHA-256 为 `100af09ca608143883e84f300cf218f5cc22a8f5b14dd985720a5da49c29c270`，包含 `arm64-v8a` 与 `x86_64` 库，已通过签名校验并安装启动于当前模拟器。
- 三端源代码提交和交付物核验完成后，才允许删除已确认的桌面/Engine 构建缓存、临时 ASCII 构建副本、过期 HAP 和旧签名副本；当前 Engine 源码、锁文件、vendor、rc.7 HAR、最终 PC/HAP、必要签名材料和用户运行数据必须保留。用户 Git 全局代理 `127.0.0.1:20808` 不得修改，只有本次进程环境中的 `127.0.0.1:9` 注入值可清除。

## 2026-08-28 重建桌面侧车与 HAP 交付记录

- 桌面端已重新执行 `npm run daemon:sidecar`、`npm run build` 和 Tauri NSIS 生产构建。便携版启动冒烟验证通过：新目录中的 `uniclipboard.exe` 能正常存活，并拉起同目录的 `uniclipd.exe`；因此交付包不再是缺少后台侧车的单文件副本。
- 本轮桌面交付物位于 `artifacts/pc/UniClipboard-1.0.0-alpha.7-retest-20260828-portable/`、其压缩包和 `UniClipboard_1.0.0-alpha.7-retest-20260828_x64-setup.exe`。旧的重复安装包、旧便携目录和压缩包已按精确路径删除；未触碰源代码、用户安装目录和运行数据。
- HarmonyOS 工程正式路径包含中文，Hvigor 返回 `00306003 Invalid project path`；已使用临时 ASCII 副本完成真实构建，日志通过 Engine rc.7 校验和 `dataTransfer` 后台模式检查，随后删除该临时副本。正式仓库的 4 个 `oh-package-lock.json5` 只因 `ohpm install` 被刷新，已恢复到构建前状态。
- 新 HAP 位于 `D:/下载/codedit/UniClipboardHarmonyOS/artifacts/sssUniClip-rc7-retest-20260828-debug-signed-acl.hap`，由当前源码构建、使用通用 `OpenHarmony.p12` 重新签名并通过 `hap-sign-tool verify-app`。它包含 `arm64-v8a` 和 `x86_64` Engine 原生库；该证书可用于模拟器，但实体机此前已返回 `9568257: fail to verify pkcs7 file`，所以本轮不能把它宣称为已通过真机安装的签名包。
- 本轮 HDC 检查时模拟器 `127.0.0.1:10000` 未在线，实体机 `192.168.1.207:12345` 也显示离线；因此没有自动向实体机安装 HAP，用户按交付路径手动安装后仍需以设备实际验签结果为准。若实体机继续拒绝，需要在 DevEco Studio 为 `com.sss.uniclipboard` 生成设备匹配的自动签名，而不是重复使用通用 OpenHarmony 证书。

## 2026-08-28 发布核验与存储清理完成

- 桌面 Release 已发布：`https://github.com/cwxsss/UniClipboardsss/releases/tag/v1.0.0-alpha.7-codex-rc7-20260828`，资产为便携版和 x64 NSIS 安装包，远端摘要与本地 SHA-256 一致。
- 鸿蒙 Release 已发布：`https://github.com/cwxsss/UniClipboardHarmonyOS/releases/tag/v1.0.5-codex-rc7-20260828`，资产为带 `READ_PASTEBOARD` 调试 ACL 的签名 HAP 和固定 Engine rc.7 HAR；两个资产均已通过 GitHub 远端摘要核验。
- 已删除并统计释放 `38,535,151,194` 字节（约 `35.889 GiB`）：桌面 `t7a`、`target`、临时工具目录，Engine `target`/OHOS 构建缓存/本地工具目录，以及鸿蒙旧 HAP 和旧签名副本。删除前再次确认所有路径位于两个项目根目录内且不含联接或其他重解析点；源码、依赖、vendor、当前 rc.7 HAR、最终交付物、必要签名材料和用户数据均保留。
- 测试用桌面进程已停止；`E:/software/UniClipboard` 中用户现有安装未触碰。工作区内的 `.codex/config.toml`、嵌套 Engine 源码仓库和 vendor 状态属于保留内容，不作为清理对象。
- 三端发布分支均以用户仓库现有 `main` 为祖先并已快进更新：桌面 `cwxsss/UniClipboardsss:main` 为 `aa402298c`，鸿蒙 `cwxsss/UniClipboardHarmonyOS:main` 为 `426ea9a`，Engine `cwxsss/Engine:main` 为 `3c81e9e`；未执行强制推送或历史改写。

## 2026-08-28 桌面端优化构建

- 针对桌面邀请二维码、历史页标题栏拖动和筛选按钮悬停反馈的未提交改动，重新执行 `npm.cmd run daemon:sidecar`、`npm.cmd run build` 和 Tauri Windows NSIS 构建；前端与后台侧车均使用当前工作树内容。
- NSIS 构建阶段已完成并生成 `target/release/bundle/nsis/UniClipboard_1.0.0-alpha.7_x64-setup.exe`。Tauri 命令最后因仓库启用更新包签名而检查到只有公钥、缺少 `TAURI_SIGNING_PRIVATE_KEY`，返回非零；这只影响更新签名产物，不影响已生成的 NSIS 安装包。
- 本次交付副本位于 `artifacts/pc/UniClipboard_1.0.0-alpha.7-desktop-fix-20260828_x64-setup.exe` 和 `artifacts/pc/UniClipboard-1.0.0-alpha.7-desktop-fix-20260828-portable.zip`；便携目录包含 `uniclipboard.exe` 与 `uniclipd.exe`。安装包 SHA-256 为 `BA194FF7A9A0AE4715D92E816C2D85DC70F87DE447344C6FD32C4A823698A11B`，便携压缩包 SHA-256 为 `DCFB673C0F486946EA702BD2A81BFCF6532F47A57578215347C659C86D6A84B7`。
- 构建仍保留已有 Rust 未使用变量、Vite 动态导入和大分包警告；本轮没有提交、推送或发布 Release。

## 2026-08-28 桌面端修复包重新交付

- 核实用户实际运行的是 `E:/software/UniClipboard/uniclipboard.exe`，该目录中的程序属于旧安装，不包含本轮邀请二维码修复；当前源码和 `dist` 已确认没有“输入口令后生成”逻辑。
- 在当前工作树重新执行侧车编译、前端构建和 Tauri NSIS 构建，生成新的桌面修复包。NSIS 安装包位于 `artifacts/pc/UniClipboard_1.0.0-alpha.7-fix-titlebar-invite-20260828_x64-setup.exe`，便携包位于 `artifacts/pc/UniClipboard-1.0.0-alpha.7-fix-titlebar-invite-20260828-portable.zip`，便携目录同时包含 `uniclipboard.exe` 和 `uniclipd.exe`。
- 新安装包 SHA-256 为 `5E93C40BB948C5C4DE0A15A620C488695BC5D95619D7E7E7AB7FB49AD5DEC606`，便携压缩包 SHA-256 为 `08BCA40A4E5B60097E4586AEEA187C9821D047A8910C506D1705348BDA26D7F4`。Tauri 最后因只有更新公钥、缺少 `TAURI_SIGNING_PRIVATE_KEY` 而返回非零，但 NSIS 包已完成生成。

## 2026-08-28 桌面邀请与历史窗口优化

- 桌面邀请二维码改为仅编码一次性邀请码。`/v2/setup/issue-invitation` 只返回邀请码和过期时间，桌面端不再要求再次输入空间口令，也不把空间口令拼入二维码；加入端仍按现有 Engine 流程输入空间口令。
- 配对完成仍以 daemon 的 `device-trust.changed` 加状态快照为准，成功态继续在 2 秒后自动关闭邀请对话框；未重新接入当前 daemon 未广播的 `setup.pairingCompleted`。
- `TitleBar`、主布局的 `ContentToolbar` 与侧栏共用 `src/hooks/useWindowDrag.ts`；历史页标题栏可拖动，筛选按钮悬停反馈与窗口控制按钮统一。
- 富文本、链接复制转文本的需求按用户确认延期，本轮未修改。
- 验证：聚焦测试 19/19；完整前端测试 1028 通过、1 跳过；TypeScript/Vite 构建和 macOS 兼容检查通过。全量 lint 仍受未跟踪 `_engine_upstream` 既有 CJS/正则规则问题影响；构建保留现有动态导入和大分包警告。

## 2026-08-28 方案一扫码协议与历史标题栏修复

- 按用户确认的方案一，桌面邀请二维码继续只编码短时邀请码，不重新加入空间口令；鸿蒙 `importSpaceInvitation` 现在接受桌面生成的仅邀请码 URI，并在扫码成功后填充邀请码、清空旧口令，加入前仍要求用户手动输入当前空间口令。
- 鸿蒙仍兼容带 `pwd` 的旧邀请 URI，以保证鸿蒙旧版本生成的二维码可用；该兼容只保留在输入解析侧，不改变桌面二维码不携带长期口令的约束。扫码成功提示已改为明确提示“请输入空间口令”。
- 桌面 `ContentToolbar` 的右侧插槽不再把整块空白区域标为不可拖动，只有插槽内明确标记的交互控件阻止窗口拖动；历史页“全部”筛选按钮和关闭状态的“搜索”按钮增加与标题栏窗口控制一致的悬停背景反馈。
- 桌面定向测试 `23/23` 通过，完整前端测试 `1031` 通过、`1` 跳过。
- 鸿蒙构建前置校验确认 Engine `v1.1.0-rc.7`、提交 `ff493cfa8563cdd7fbf8615ed0a95b9058714176` 和 `dataTransfer` 后台同步模式通过。由于 DevEco 不接受中文工程路径，本轮在 ASCII 临时副本中运行测试和 debug HAP 构建；两项任务均被仓库已有的 ArkTS 严格类型错误以及 `@uniclipboard/engine` 的 `Index.d.ets` 命名导出识别问题阻断，错误未落在本轮新增的扫码解析代码上，因此未宣称生成新的 HAP。临时验证副本不属于项目交付物。
- 当前仍处于线上内测与快速迭代阶段，Engine HAR/NAPI 声明导出和既存 ArkTS 严格类型问题记录为后续构建阻塞；在修复前不应把鸿蒙 HAP 构建结果标记为通过。

## 2026-08-28 中继验证与桌面安装包交付规则

- 后续桌面交付只生成 NSIS 安装包，不再生成新的便携版目录或压缩包。当前工作树生成的安装包位于 `artifacts/pc/UniClipboard_1.0.0-alpha.7-relay-test-20260828_x64-setup.exe`，SHA-256 为 `1F24CE860FD1257A97431379B5052886F94057E7AC7C056E3CEFFE735B479657`；构建使用当前工作树和已编译的 `uniclipd-x86_64-pc-windows-msvc.exe` 侧车。
- 桌面设置中的 `https://relay.chatsss.top` 已被 Engine 选为当前中继，日志出现 `home is now relay https://relay.chatsss.top/`；从 Engine 直接运行的真实 iroh 中继协议探测也已通过。中继网页根路径返回 404 属于服务路由预期，不代表中继协议不可用。
- 邀请服务与中继是两套地址：邀请码注册/兑换走 rendezvous 服务，中继只承载 iroh 数据连接。对 rendezvous 的解析/消费接口探测可达并返回结构化 `pairing_not_found`，说明接口路径和服务可访问。
- 旧桌面日志显示，远端已通过 `/uniclipboard/pairing/2` 连接到桌面，但在打开配对双向流、发送第一帧之前由对端关闭连接；当前证据不支持把问题归因于中继不可达，更可能是手机端未完成同一 Engine 配对握手、版本/运行状态不一致或邀请码流程已失效。物理手机 HDC 当前离线，尚未取得手机侧日志，暂不修改配对代码。
- 当前 Codex 进程曾注入 `127.0.0.1:9` 代理变量；网络验证时仅在进程级清除。用户 Git 全局代理 `127.0.0.1:20808` 已保留且不得修改。
- 待用户安装当前 NSIS 包后，使用全新邀请码在跨网络环境复测，并同时取得桌面和手机日志；只有在双端时间线确认后才进入中继/配对代码修复。

## 2026-08-28 模拟器配对与中继测试边界

- 使用当前 `v1.1.0-rc.7` Engine 的鸿蒙模拟器完成一次完整配对：桌面端接受配对双向流、解码首帧、验证加入端证明并发送持久化入会候选，随后将模拟器标记为在线；因此本次配对链路已成功。
- 鸿蒙模拟器保存的自定义中继 `https://relay.chatsss.top` 在强制停止并重启后仍然存在，桌面端启动日志也确认已加载该中继。Engine 的策略仍是直连优先、中继回退；“已配置中继”不等于“当前会话强制走中继”。
- 模拟器与桌面端同机运行时，直连是预期路径，不能作为中继实测。有效的中继验证需要手机与桌面处于不同网络，或在明确授权后临时阻断直连并保留中继；验证时必须同时检查双端通道日志，不能只看界面状态。

## 2026-08-28 当前源码 HAP 重新构建

- 之前 `07:55:49` 生成的 `sssUniClip-rc7-retest-20260828-debug-signed-acl.hap` 不是当前源码最新构建；鸿蒙端四个未提交源码文件在 `09:51` 仍有修改，产品锁文件也在 `08:00` 更新。
- DevEco/Hvigor 直接使用中文项目路径会报 `00306003 Invalid project path`。本次未修改源码，将当前工作树复制到纯英文临时路径后重新执行 `ohpm install --all` 与 `assembleHap`，构建日志包含 Engine `v1.1.0-rc.7` 校验和 `BUILD SUCCESSFUL`。
- 重新生成的 HAP 使用本机 OpenHarmony 测试签名并通过 `verify-app`，同时包含 `arm64-v8a` 与 `x86_64` 原生库。交付文件为 `UniClipboardHarmonyOS/artifacts/sssUniClip-rc7-current-source-20260828-debug-signed-acl.hap`，生成时间 `11:07:18`，SHA-256 为 `3E6B1C8565F5273046C6F68188D1B28E98D645607A99DD243EDC797FAC1AFEB0`。

## 2026-08-28 剪贴板重复记录与回环抑制修复

- 用户在 HP 笔记本复制“关于我们”后，当前桌面 `zsmy` 在约 7 秒内收到多个不同的 `snapshot_hash`，但明文长度始终为 14 字节，HTML 表示长度按 4 字节递增；这些事件均由 `RemotePush` 进入，说明是同一可见文本的富文本表示重写被当成多个物理快照，而不是历史界面重复渲染。
- 桌面多空间 Hub 最近为按 Windows 序列号消费自身写回回声而使用 `new_passthrough`，绕过了平台监听器原有的内容去重。现改为带事件过滤回调的监听器：先按变化令牌消费程序写回，再保留普通内容去重，并在 15 秒内合并相同可见文本的富文本表示变化；不同真实变化令牌的完整快照仍可通过。
- Engine 入站去重窗口从 2 秒调整为 15 秒，以覆盖日志中约 5 秒后的延迟表示重写；命中可见内容后同时刷新哈希和可见内容缓存。对已存在条目的重复激活新增 `(snapshot_hash, activated_at_ms)` 守卫，避免同一活动状态在约几百毫秒的重发中反复写回系统剪贴板并再次触发同步；真正重新复制相同内容会带新的激活时间，仍可重新激活。
- 验证通过：桌面 `uc-platform` 监听器测试 26/26，`uc-bootstrap` 多空间 Hub 测试 18/18；Engine `uc-application` 入站同步测试 84/84，Engine 格式检查和桌面格式检查均通过。Engine 测试仍会打印既有的后台 Mock 写入期望提示，但进程结果为成功，本轮没有出现新增失败。
- 依赖边界：桌面根工程当前仍固定解析远端 Engine 提交 `ff493cfa8563cdd7fbf8615ed0a95b9058714176`；本轮 Engine 修复位于工作区内独立的 `_engine_upstream` 仓库，尚未推送或改变桌面 `Cargo.toml` 的远端提交，因此发布桌面包前必须先将 Engine 修复提交到选定远端并更新桌面锁定提交，再做一次完整构建核验。
- 本轮没有打包、推送或发布 Release；暂不扩大到图片/文件策略或生产安全专项，真实多设备链路仍需在更新后的桌面包上复测重复历史和回环行为。

## 2026-08-28 新桌面安装包构建

- 按用户要求仅生成 Windows x64 NSIS 安装包，没有生成便携版。因当前 PowerShell 环境未提供 `bun`，先直接运行前端生产构建，再使用临时 Tauri 配置跳过重复的前端构建步骤；临时配置已在构建完成后删除。
- 后台 `uniclipd` 侧车使用 `x86_64-pc-windows-msvc` release 构建并成功暂存；前端 `tsc`、Vite 生产构建和 macOS 兼容检查均通过。Vite 仍报告既有的大分包和动态导入提示，未阻断构建。
- 新安装包位于 `target/release/bundle/nsis/UniClipboard_1.0.0-alpha.7_x64-setup.exe`，生成时间为 `2026-08-28 13:23:01`，大小 `19,498,283` 字节，SHA-256 为 `434E2C8D31074A22FDD116428C794885EC9304C051413ECED850D53EFCE6027E`。
- Tauri 最后因配置了更新公钥但当前环境没有 `TAURI_SIGNING_PRIVATE_KEY` 而返回非零；NSIS 安装包已完整生成，该错误只影响更新签名产物。本轮未推送代码、未发布 Release、未删除已有安装包或用户数据。

## 2026-08-28 鸿蒙后台耗电优化实施

- 按确认方案修改鸿蒙端：`common/src/main/ets/engine/EngineClipboardHost.ets` 将固定 `200ms` 轮询改为事件监听加 `1s -> 2s -> 5s` 自适应兜底，改用单个递归 `setTimeout`，剪贴板发生变化后重新回到快速检查；未改变 `dataTransfer` 后台任务。
- `features/clipboard/src/main/ets/viewmodel/ClipboardFeatureController.ets` 在页面隐藏和 Ability 进入后台时清除界面层实时轮询，调度函数增加页面可见性守卫，避免异步查询收尾重新创建定时器；前台恢复时立即对账并刷新一次设备列表，删除每 4 次轮询刷新设备的旧路径。
- `products/default/src/main/ets/entryability/EntryAbility.ets` 接入控制器的后台/前台生命周期，确保即使页面回调时序变化也不会在后台继续执行界面轮询。新增轮询退避和后台定时器清理测试。
- 使用纯英文临时目录运行 `assembleHap --mode module -p product=default -p module=entry@default -p buildMode=release --no-daemon`，`CompileArkTS`、`SignHap`（当前无产品签名配置）和 `BUILD SUCCESSFUL` 均通过，Engine rc.7 与 `dataTransfer` 校验通过。临时目录已按精确路径删除，未修改正式构建产物。
- 全量 `test` 当前仍被既有测试基础设施阻断：产品测试模板引用不存在的 `products/default/src/test/List.test`，公共模块测试另有 `EngineRuntimeService.test.ets` 的 3 处未类型化对象字面量错误；这些问题未落在本轮耗电实现上，待后续单独修复。
- 基于同一当前源码再次生成并签名验证测试 HAP：`UniClipboardHarmonyOS/artifacts/sssUniClip-rc7-power-optimized-20260828-debug-signed-acl.hap`，大小 `191,137,469` 字节，SHA-256 为 `2A7CFDDAA8DF56F23B4A0BC5695EF0B9279E2B8CF62C3CA691E05D8A799D8EE0`。该包使用 OpenHarmony 测试签名，仅用于内测耗电和同步回归，不宣称为正式发布签名。

## 2026-08-28 鸿蒙后台耗电审计

- 只读检查确认：鸿蒙端在后台同步开启且已加入空间时，`common/src/main/ets/engine/EngineClipboardHost.ets` 会以 `200ms` 固定间隔调用 `systemPasteboard.getDataSync()`，约每秒读取 5 次；系统 `pasteboard` 更新监听同时保持注册。该轮询最初为降低后台文字同步延迟而引入，现有测试也把 `200ms` 当作延迟上限，而不是耗电指标。
- 发现第二套重复轮询：`features/clipboard/src/main/ets/viewmodel/ClipboardFeatureController.ets` 的页面控制器在 `onPageHide()` 后只把页面标记为不可见，没有停止定时器；后台同步开启时仍约每 2 秒调用一次 Engine 入站查询，并每 4 次刷新设备列表。控制器已经订阅 Engine 事件，因此该定时器具备收敛为前台兜底或事件丢失时重试的条件，但需实机验证事件通道可靠性。
- `products/default/src/main/module.json5` 与 `BackgroundSyncService.ets` 的 `dataTransfer` 后台任务属于长时间接收的必要能力。README 已记录改成 `multiDeviceConnection` 会在约 65 秒后被系统挂起，不能用切换后台模式的方式换取续航。
- 多空间监督器会为每个启用且已加入的非当前空间启动独立 Engine 运行时；多个空间会线性增加网络保活、事件等待和重连成本。是否默认保持所有空间后台接收属于产品策略，暂不擅自改变。
- 建议的下一步：保留 `dataTransfer`，将 Engine 剪贴板检测改为事件优先、带退避的低频兜底；应用进入后台时暂停界面控制器轮询，恢复前台时执行一次立即对账；再用真机对后台 30 分钟的剪贴板读取次数、CPU、网络、耗电和文字同步延迟做基准。该建议尚未改动鸿蒙源码，等待用户确认后实施。

## 2026-08-28 首次设置与快捷键默认值修复

- 首次设备创建空间成功后，`InitializeSpaceScreen` 直接完成设置流程并导航到主界面，不再停留在“空间已准备好”的完成页；主界面的普通设备邀请不会再次打开首次设置引导。
- 快捷键默认值统一为仅保留切换快捷面板 `Alt+V`，设置、缩放、收藏和历史搜索等其他快捷键默认为空；空绑定不会注册运行时监听器，用户后续手动配置仍可生效。历史页搜索与收藏操作改为读取统一定义，避免代码内残留旧默认快捷键。
- 验证通过：完整前端 Vitest `157` 个测试文件通过，`1035` 个测试通过、`1` 个跳过；定向 Vitest `19/19`，相关源码与测试的 `oxlint`、`oxfmt`、`git diff --check` 通过；前端生产构建的 TypeScript、Vite 和 macOS 兼容性检查通过。构建仍保留既有的动态导入和大分包提示。

## 2026-08-28 桌面端首次设置与快捷键修复包

- 使用当前工作树重新执行前端生产构建、`uniclipd` Windows release 侧车构建和 Tauri NSIS 打包；前端 TypeScript、Vite、macOS 兼容性检查，以及后台 release 编译均通过。
- 最新 Windows x64 NSIS 安装包位于 `target/release/bundle/nsis/UniClipboard_1.0.0-alpha.7_x64-setup.exe`，生成时间为 `2026-08-28 15:38:16`，大小 `19,502,997` 字节，SHA-256 为 `BA839CC3DEEB61A2905F53EF0FDF7AFD577EFF41F4A17E020259F4C3E5202256`。主程序为 `target/release/uniclipboard.exe`，后台侧车已暂存到 `src-tauri/binaries/uniclipd-x86_64-pc-windows-msvc.exe`。
- 本次只生成 NSIS 安装包，没有生成便携版；本地打包使用临时 Tauri 配置跳过本机缺失的 `bun` 前端钩子，并关闭更新签名产物生成，未修改正式 Tauri 配置。安装包可用于当前 Windows x64 内测安装，更新包签名不属于本次交付。

## 2026-08-28 三端代码同步到用户 GitHub main

- 桌面端已将当前已核对的源代码、测试和项目文档提交为 `ff5db6a6f53e61e5d2bfe982d39af49128f70c22`，推送到 `cwxsss/UniClipboardsss` 的 `main`。
- Engine 已将入站剪贴板去重修复提交为 `44329b55d2b420e195b8e421401897cbc31dff62`，推送到 `cwxsss/Engine` 的 `main`。
- 鸿蒙端已将后台剪贴板轮询优化提交为 `ef5d26d601dfaba0cdaa9f79ed7e9a58abc93765`，推送到 `cwxsss/UniClipboardHarmonyOS` 的 `main`。三个远端提交均已通过 GitHub CLI 独立核验。
- 桌面端提交时仓库 `pre-commit` 仅调用本机不存在的 `bun lint-staged`，已在既有测试、格式检查和生产构建通过的前提下跳过该钩子；未修改钩子文件。用户 Git 配置中的 `127.0.0.1:20808` 保持不变，仅清除了本次 Git 子进程中的临时 `127.0.0.1:9` 代理环境变量。

## 2026-08-28 Windows 剪贴板转发器自动恢复

- 用户报告空间连接后桌面端必须重启才能再次捕获本机剪贴板。实时日志确认重启后的 daemon 能正常捕获并分发文本，因此不是 Windows 剪贴板权限或 Engine 配对未完成。
- 根因位于 `apps/daemon/src/daemon/production_spaces.rs`：多空间 `ClipboardForwarder` 在任意一次 `router.clipboard_changed()` 失败时通过 `?` 退出，而 `DesktopClipboardHub` 的唯一物理监听流只在 daemon 启动时取得一次，退出后没有重新取得机制。刚重连时的临时发送失败因而会永久停止后续本机复制处理，直至重启 daemon。
- 修复后，单个快照的路由/发送失败仅记录不含正文的结构化告警并继续监听；监听流关闭或错误时，转发器关闭旧流、等待其租约释放并以 250ms 有界间隔重新取得唯一监听器。首次启动本就没有系统监听器时仍保持原有禁用语义，不会创建无意义的重试循环。
- 新增两项 daemon 回归测试：一次分发失败后下一次快照仍会送达路由器；监听流异常后会自动重新取得监听器。`cargo test -p uc-daemon` 通过 `123` 项单元测试和 `10` 项接口契约测试，`git diff --check` 通过。随后已生成并安装包含该修复的新桌面构建，具体交付验证记录见下一节。

## 2026-08-28 Windows 剪贴板转发器构建、安装与启动验证

- 使用 `npm.cmd run daemon:sidecar`、`npm.cmd run build` 与仅覆盖本机打包行为的临时 Tauri 配置重新生成 Windows x64 NSIS 安装包；临时配置已经删除。`target/release/bundle/nsis/UniClipboard_1.0.0-alpha.7_x64-setup.exe` 于 `2026-08-28 23:05:04` 生成，大小 `19,500,678` 字节，SHA-256 为 `91D089E8007E675F809E67685C737CBE70046A4F30F4FBF850911D86A1179E2F`。本机构建未生成便携版，也未生成需要私钥的在线更新签名产物。
- 该 NSIS 包已先静默安装到项目内隔离目录并成功启动主程序与对应 `uniclipd` 侧车，随后已覆盖安装至 `E:/software/UniClipboard`；实际实例于 `2026-08-28 23:10` 启动并拉起新侧车。已安装的 `uniclipboard.exe` SHA-256 为 `D1065F97E6A2CE16E3EE322FCDAB19CF65490CC947E11B3BEA1362D8E4AA75DA`，`uniclipd.exe` SHA-256 为 `CD881A451F6F7EFD3BE41EF0F845A8F234681BFC76F8F9276371CA681B518318`。
- 为避免覆盖用户正在使用的系统剪贴板，安装后仅验证了 GUI 与 daemon 的实际启动，没有自动写入测试文本；后续应在已连接空间中由用户复制一段任意文本，确认断线/重连后无需再重启桌面程序即可被捕获。
