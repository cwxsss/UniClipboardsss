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
