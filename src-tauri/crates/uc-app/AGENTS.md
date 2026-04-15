# `uc-app/AGENTS.md`

## 1. 文档目的

`uc-app` 是 UniClipboard 的应用层，负责把 `uc-core` 中定义的领域能力，组织成可执行的用例、流程和对外服务接口。

本规范用于约束所有开发者和 AI Agent 在修改 `uc-app` 时的设计与编码行为，确保：

* 应用层只做流程编排，不定义底层业务真相
* 不侵入 `uc-core` 的领域边界
* 不下沉到 `uc-infra` 的实现细节
* 用例、状态机、orchestrator、facade 职责清晰
* 对 UI / CLI / daemon API 提供稳定、明确、可演化的应用接口

**任何修改 `uc-app` 的提交，都必须遵循本规范并完成自我审查。**

---

## 2. `uc-app` 的定位

### 2.1 核心职责

`uc-app` 只负责以下事情：

1. **用例编排**

   * 将多个 domain object、ports、policies 串起来完成一个用户可感知动作

2. **应用流程控制**

   * 例如 setup、join space、clipboard capture、sync、search 等流程推进

3. **状态机与流程状态管理**

   * 管理复杂的应用阶段与状态转换
   * 但状态机表达的是流程，而不是底层技术协议

4. **事务边界与一致性协调**

   * 决定某个 use case 内哪些步骤属于同一次应用动作

5. **面向上层提供 facade / application service**

   * 给 UI、CLI、daemon API 提供稳定接口
   * 隐藏 `uc-core` 和 `uc-infra` 的细节

6. **应用层事件协调**

   * 处理来自 infra / core 的信号，并驱动流程推进

---

### 2.2 非职责

以下内容**不属于** `uc-app`：

| 类别      | 示例                                          |
| ------- | ------------------------------------------- |
| 领域真相定义  | `ClipboardEntry` 的核心业务规则                    |
| 基础设施实现  | SQLite、libp2p、文件系统、HTTP client              |
| 平台细节    | AppData、Windows Clipboard、Keychain          |
| 表示层逻辑   | HTTP DTO、前端 ViewModel、Tauri command request |
| 启动装配    | wiring、bootstrap、main 入口                    |
| 第三方类型传播 | 直接暴露 `sqlx::Error`、`libp2p::PeerId`         |

---

## 3. 分层关系

```text
UI / CLI / Daemon API
        ↓
      uc-app
        ↓
      uc-core
        ↑
      uc-infra
```

### 强制规则

* `uc-app` **可以依赖** `uc-core`
* `uc-app` **可以依赖** `uc-core` 中定义的 ports
* `uc-app` **不可以依赖**具体 infra 实现类型
* `uc-app` **不可以自己重新定义领域真相**
* `uc-app` **不可以承担表示层职责**

---

## 4. 应用层的核心原则

## 4.1 编排，而不是定义业务真相

`uc-app` 的职责是把已有业务能力组织起来，而不是重新发明业务规则。

错误示例：

* 在 use case 中重新定义“什么叫已配对”
* 在 orchestrator 中私自决定“哪些记录应同步”
* 在 façade 中直接构造业务规则，绕开 core policy

正确做法：

* 业务规则归 `uc-core`
* 应用层只负责“什么时候调用、按什么顺序调用、如何汇总结果”

---

## 4.2 面向用例，不面向技术堆栈

`uc-app` 里的模块应围绕用户动作和业务流程组织，而不是围绕技术实现组织。

推荐：

* `setup`
* `space_access`
* `clipboard_capture`
* `clipboard_sync`
* `search`
* `settings`
* `facade`

不推荐：

* `database_logic`
* `network_flow`
* `http_service`
* `libp2p_orchestrator`

---

## 4.3 上层只看见“应用动作”和“应用状态”

UI / CLI / daemon API 不应直接理解：

* 多个 repository 的调用顺序
* 多个 port 的协调方式
* 复杂的底层 domain 细节拼装
* infra 失败细节

`uc-app` 应对上层暴露：

* 清晰的命令
* 清晰的查询
* 清晰的状态
* 清晰的错误语义

---

## 4.4 应用层必须显式承担流程复杂性

复杂性不能靠“散在各个 handler 里”来消化。

当一个流程涉及：

* 多步骤推进
* 用户输入
* 异步事件
* 超时
* 取消
* 外部系统结果回调

就应该明确建模为：

* state machine
* orchestrator
* coordinator
* application service

而不是几个函数随手串一下。

---

## 5. `uc-app` 中允许包含的内容

### 5.1 Use Case

例如：

* `StartNewSpaceUseCase`
* `StartJoinSpaceUseCase`
* `SubmitSpacePassphraseUseCase`
* `CaptureClipboardUseCase`
* `SearchClipboardUseCase`
* `UpdateSettingsUseCase`

---

### 5.2 Orchestrator / Coordinator

适用于跨多个 port、多个阶段的长流程。

例如：

* `SetupOrchestrator`
* `FileTransferOrchestrator`
* `SpaceAccessOrchestrator`

---

### 5.3 State Machine

适用于流程状态显式化。

例如：

* onboarding / setup 状态机
* pairing 状态机
* transfer 会话状态机

---

### 5.4 Application Facade

用于向 UI / CLI / daemon API 提供稳定入口。

例如：

* `SetupFacade`
* `ClipboardFacade`
* `SearchFacade`

---

### 5.5 Command / Query Model

面向上层应用调用的输入输出模型。

注意：这里是**应用层模型**，不是 HTTP DTO，也不是数据库 model。

---

### 5.6 应用层错误

例如：

* `SetupError`
* `ClipboardCaptureError`
* `SearchError`

这些错误应表达**应用动作失败**，而不是底层库错误。

---

## 6. `uc-app` 中禁止包含的内容

## 6.1 禁止直接写 infra 实现逻辑

不允许在 `uc-app` 中：

* 直接访问 SQLite schema
* 直接读写文件
* 直接操作 libp2p protocol
* 直接使用平台 API
* 直接使用加密算法库

`uc-app` 必须通过 `uc-core` 定义的 port 与外界交互。

---

## 6.2 禁止直接暴露表示层模型

不允许把这些东西放进 `uc-app` 作为公共真相：

* HTTP request / response DTO
* Tauri command 入参 / 出参
* 前端页面状态对象
* CLI 参数 parser 类型

这些属于表示层或入口层。

---

## 6.3 禁止把应用层写成“巨型 service”

高度警惕以下命名：

* `AppService`
* `SystemService`
* `GlobalCoordinator`
* `MainUseCase`
* `EverythingFacade`

如果一个对象同时做：

* 状态机
* repository 调用
* cache
* 日志聚合
* 事件发布
* UI 输出转换

说明已经失控，必须拆分。

---

## 6.4 禁止让 `uc-app` 反向定义 `uc-core`

错误方向：

* 因为某个 use case 写起来方便，就给 domain 塞奇怪字段
* 因为 UI 需要一个状态，就把 UI 状态直接写进 core entity
* 因为 API 返回需要，就修改 core model 结构

正确方向：

* app 适配 core
* 上层也适配 app
* 不让 core 为某一层临时需求变形

---

## 7. 模块组织规范

推荐按业务流程和能力组织：

```text
uc-app/
  src/
    setup/
      mod.rs
      facade.rs
      commands.rs
      queries.rs
      orchestrator.rs
      state_machine.rs
      state.rs
      events.rs
      errors.rs

    clipboard_capture/
      mod.rs
      usecase.rs
      commands.rs
      errors.rs

    clipboard_sync/
      mod.rs
      orchestrator.rs
      session.rs
      events.rs
      errors.rs

    search/
      mod.rs
      facade.rs
      usecase.rs
      query.rs
      result.rs
      errors.rs

    settings/
      mod.rs
      facade.rs
      usecase.rs
      commands.rs
      errors.rs

    shared/
      mod.rs
      pagination.rs
      application_event.rs
      trace.rs
```

不推荐按“技术角色”切碎，例如：

```text
services/
repositories/
managers/
helpers/
handlers/
```

这种结构后面很容易失焦。

---

## 8. Use Case 设计规范

## 8.1 一个 use case 表达一个明确动作

一个 use case 应该回答：

> “用户或系统到底想完成什么？”

例如：

* 启动新空间
* 提交加入空间口令
* 捕获当前剪切板
* 查询最近记录
* 更新设置

而不是：

* `HandleClipboard`
* `ProcessData`
* `DoSetup`

---

## 8.2 Use Case 应显式定义输入与输出

不要让 use case 靠“读全局状态 + 改全局状态”工作。

推荐：

```rust
pub struct StartJoinSpaceCommand {
    pub sponsor_device_id: DeviceId,
}

pub struct StartJoinSpaceResult {
    pub status: SetupStatus,
}
```

这样边界清晰，也方便测试。

---

## 8.3 Use Case 应避免承担长生命周期状态

短动作适合 `UseCase`。

长流程适合：

* `Orchestrator`
* `StateMachine`
* `Coordinator`

如果一个 use case 要记住：

* 当前步骤
* 等待用户输入
* 超时
* 异步回调
* 重试次数

那通常已经不是单纯 use case 了。

---

## 9. Orchestrator 设计规范

## 9.1 Orchestrator 的职责是“推进流程”

适用于：

* 多步流程
* 事件驱动推进
* 需要维护应用状态
* 跨多个 port 和 domain object

例如：

* setup
* pairing
* file transfer session
* join flow

---

## 9.2 Orchestrator 不是万能垃圾桶

不允许一个 orchestrator 同时负责：

* 用户输入校验
* 全部业务规则判断
* repository 实现细节
* UI 显示状态组装
* API DTO 转换
* 所有日志与监控格式拼装

Orchestrator 只做一件事：

> **在应用层推进一个复杂流程**

---

## 9.3 Orchestrator 必须显式定义状态与事件

如果是长流程，必须明确：

* 当前状态是什么
* 接收什么事件
* 每个事件下允许哪些转移
* 哪些动作在转移时触发
* 哪些错误可恢复，哪些不可恢复

不允许靠大量布尔值和 if-else 链撑流程。

---

## 10. State Machine 设计规范

## 10.1 状态机表达的是应用流程，不是协议实现

例如 `setup` 状态机可以表达：

* Idle
* WaitingForDeviceSelection
* WaitingForPassphrase
* WaitingForProof
* Completed
* Failed

但不应直接表达：

* libp2p stream open
* websocket frame ack
* tcp reconnecting

这些是 infra 细节。

---

## 10.2 状态机必须有单一真相来源

UI、CLI、daemon API 应从统一的 `ApplicationStatus` 或流程状态模型读取状态。

不允许：

* UI 自己推断一半
* handler 自己判断一半
* orchestrator 自己藏一半
* infra 事件里再带一半

状态真相必须单点收口。

---

## 10.3 状态机必须显式支持失败、取消、超时

不能只建 happy path。

必须考虑：

* 用户取消
* 对端断开
* 超时
* 输入错误
* 中途重试
* 资源不可用
* 会话失效

---

## 11. Facade 设计规范

## 11.1 Facade 是给上层的稳定入口

Facade 的目标：

* 降低 UI / CLI / daemon API 对内部结构的理解成本
* 隐藏多个 use case / orchestrator / repository 的协调细节
* 提供一致的命令与查询接口

例如：

* `get_status()`
* `start_new_space()`
* `start_join_space()`
* `submit_passphrase()`
* `cancel()`

---

## 11.2 Facade 不应重新承载复杂业务流程

Facade 应该是入口，不应成为另一个巨型 orchestrator。

Facade 内部可以调用：

* use case
* orchestrator
* query service

但不应自己塞满：

* 状态流转逻辑
* 多阶段流程细节
* 大量领域判断

---

## 11.3 Facade 输出应面向应用语义，而不是领域内部细节

对 UI 暴露：

* `SetupStatus`
* `SearchResultPage`
* `ClipboardPreview`

而不是直接暴露：

* 十几个 domain object 拼装结果
* 底层 repository raw data
* infra event 原始消息

---

## 12. 命令、查询与结果模型规范

## 12.1 命令模型与查询模型分离

不要混用一个对象既做“命令输入”又做“查询结果”。

推荐：

* `StartJoinSpaceCommand`
* `GetSetupStatusQuery`
* `SearchClipboardQuery`
* `SearchClipboardResult`

---

## 12.2 应用模型不等于 DTO

应用模型是 `uc-app` 对外的稳定接口语义。

它不应该直接等于：

* HTTP JSON schema
* Tauri invoke payload
* CLI 参数结构

这些都应在更外层适配。

---

## 12.3 结果模型应优先表达“上层真正关心的内容”

不要把底层细节全抛给 UI 再自己拼。

例如 Setup 状态，UI 真正关心的是：

* 当前阶段
* 是否需要用户输入
* 是否可取消
* 错误提示类型
* 下一步动作

而不是：

* 所有低层事件日志
* 所有底层 port 响应对象

---

## 13. 错误处理规范

## 13.1 应用层错误必须表达“动作失败语义”

不允许把底层错误直接当作应用错误。

错误示例：

* `sqlx::Error`
* `std::io::Error`
* `libp2p::TransportError`

正确示例：

* `SetupError::PairingUnavailable`
* `SetupError::InvalidPassphrase`
* `CaptureClipboardError::PersistenceFailed`
* `SearchError::IndexUnavailable`

---

## 13.2 应用层必须做错误翻译与归类

`uc-app` 是非常重要的“错误收口层”。

它应把：

* core 错误
* infra 错误
* 流程错误

翻译成上层可理解的动作语义。

---

## 13.3 禁止静默吞错

不允许：

* 某步失败后继续当没事
* 异步任务失败但不更新状态
* 错误被 log 一下就结束
* 返回“空结果”伪装成功

应用层失败必须要么：

* 显式进入失败状态
* 显式返回错误
* 显式触发补救流程

---

## 14. 事件处理规范

## 14.1 应用层可以处理事件，但事件必须服务于流程推进

事件不是为了“哪里都能发点东西”。

只保留两类有价值的事件：

* 驱动流程推进的事件
* 对状态一致性有帮助的事件

---

## 14.2 应用层事件必须有明确来源和去向

回答清楚：

* 这个事件是谁发的
* 谁消费
* 触发什么状态变化
* 是否幂等
* 重复到达怎么办
* 丢失怎么办

---

## 14.3 不允许事件泛滥替代结构化设计

不要把一切都做成 event bus 然后谁都能监听。

否则最后很容易变成：

* 隐式耦合
* 难以测试
* 难以追踪
* 状态来源不清

---

## 15. 并发、异步与后台任务规范

## 15.1 应用层可以管理后台流程，但必须可控

所有后台任务都必须：

* 可追踪
* 可取消
* 可观察失败
* 有明确生命周期
* 与应用状态同步

不允许“spawn 了就不管”。

---

## 15.2 异步流程失败必须回写应用状态

例如：

* pairing loop 崩了
* file transfer session 中断了
* background watcher 停了

必须反映到：

* 应用状态
* facade 查询结果
* 日志与 tracing

---

## 15.3 不允许把运行时细节扩散到业务模型

例如不要因为用了 tokio，就让 core/app 模型里充满：

* channel sender
* join handle
* runtime handle
* oneshot receiver

这些应尽量收敛在 orchestrator 或内部实现中。

---

## 16. 日志与 tracing 规范

## 16.1 `uc-app` 必须可观测

关键流程必须有 tracing，尤其是：

* setup
* join
* pairing
* clipboard capture
* sync
* search

至少覆盖：

* 命令入口
* 状态变化
* port 调用前后
* 失败路径
* 重试路径
* 取消路径
* 超时路径

---

## 16.2 日志要面向流程排障

日志要能回答：

* 当前是什么 use case / orchestrator
* 输入命令是什么
* 当前状态是什么
* 触发了什么转移
* 调用了哪些 ports
* 哪一步失败
* 失败后进入了什么状态

---

## 16.3 不得泄露敏感数据

严禁打印：

* 明文剪切板内容
* 明文 passphrase
* 明文密钥
* 大段用户私有 payload

允许打印：

* id
* type
* size
* 状态
* hash 截断
* 错误类别

---

## 17. 测试规范

## 17.1 `uc-app` 的核心测试对象是“流程正确性”

测试重点不是某个函数有没有调用，而是：

* 用例是否按预期推进
* 状态机是否覆盖正确
* 错误时是否进入正确状态
* 超时 / 取消 / 重试是否行为正确

---

## 17.2 Use Case 测试必须围绕应用动作

例如：

* 调用 `start_join_space()` 后状态是否变成 `WaitingForSelection`
* 提交 passphrase 后是否进入 `WaitingForProof`
* proof 失败后是否进入 `Failed(InvalidPassphrase)`

---

## 17.3 Orchestrator / State Machine 必须重测异常路径

必须覆盖：

* 非法状态转移
* 重复事件
* 延迟事件
* 取消后又收到回调
* 超时后又收到成功结果
* 幂等性场景

---

## 17.4 测试依赖 port mock，而不是 mock 具体 infra

`uc-app` 测试应依赖：

* `uc-core` port mock
* fake repository / fake service

而不是直接依赖 sqlite/libp2p 等具体实现。

---

## 18. 命名规范

| 类型           | 规则              | 示例                        |
| ------------ | --------------- | ------------------------- |
| Use Case     | `*UseCase`      | `CaptureClipboardUseCase` |
| Orchestrator | `*Orchestrator` | `SetupOrchestrator`       |
| Facade       | `*Facade`       | `SetupFacade`             |
| Command      | `*Command`      | `StartJoinSpaceCommand`   |
| Query        | `*Query`        | `GetSetupStatusQuery`     |
| Result       | `*Result`       | `SearchClipboardResult`   |
| Error        | `*Error`        | `SetupError`              |
| State        | `*State` / 具体枚举 | `SetupState`              |

避免模糊命名：

* `Manager`
* `Service`
* `Processor`
* `Handler`
* `Coordinator`（除非真的在做协调）

---

## 19. 提交前自我审查清单

每次修改 `uc-app`，必须逐项自查：

### 19.1 边界检查

* [ ] 这次修改是否属于应用层编排，而不是 core 业务真相或 infra 实现？
* [ ] 是否绕过 port 直接依赖了 infra 具体实现？
* [ ] 是否引入了 HTTP / Tauri / CLI / 前端表示层模型？

### 19.2 用例检查

* [ ] 这个模块是否明确表达了一个应用动作或流程？
* [ ] 是否存在职责过大的 use case / orchestrator / facade？
* [ ] 是否把长流程与短动作正确区分？

### 19.3 状态检查

* [ ] 是否有统一状态真相来源？
* [ ] 是否考虑了取消、失败、超时、重复事件？
* [ ] 是否存在 UI 自己推断流程状态的风险？

### 19.4 错误检查

* [ ] 是否把底层错误翻译成了应用层错误？
* [ ] 是否存在静默吞错？
* [ ] 后台流程失败是否能被状态和日志感知？

### 19.5 可观测性检查

* [ ] 是否为关键流程增加了 tracing？
* [ ] 日志是否足以排查状态推进过程？
* [ ] 是否避免打印敏感数据？

### 19.6 测试检查

* [ ] 是否覆盖 happy path 以外的状态流转？
* [ ] 是否测试了取消、超时、重复输入、异常回调？
* [ ] 是否依赖 port mock 而不是具体 infra？

---

## 20. Code Review 重点

评审 `uc-app` 时，优先检查：

1. 是否越权定义了 core 规则
2. 是否直接依赖具体 infra
3. 是否把 façade / orchestrator 写成上帝对象
4. 是否状态机不完整
5. 是否错误未收口
6. 是否后台任务不可控
7. 是否上层接口暴露了过多内部细节

---

## 21. 反模式清单

### 21.1 Use Case 变成“万能函数”

一个 use case 同时负责：

* 参数解析
* 业务判断
* repository 访问
* UI 输出拼装
* telemetry 汇总

这说明边界已经坏了。

---

### 21.2 Facade 变成“第二个 app 层”

Facade 应该薄而稳定，不应成为另一个巨型协调中心。

---

### 21.3 状态分散在多个地方

* orchestrator 有一份
* UI 自己推一份
* handler 再拼一份
* repository 里还藏一份

这是最危险的失控源。

---

### 21.4 为了好写，直接把 infra 类型往上带

例如：

* 用 libp2p peer id 直接做应用状态
* 用 DB row 直接当结果返回
* 用 HTTP DTO 直接当 use case 输入

都会导致长期耦合。

---

### 21.5 事件总线化一切

看到什么都发事件、到处监听，最后流程不可追踪。

---

## 22. 总原则

`uc-app` 必须遵守这四条：

### 22.1 不定义业务真相，只组织业务动作

### 22.2 不实现基础设施，只依赖抽象 port

### 22.3 不暴露内部复杂性，只输出稳定应用接口

### 22.4 不回避流程复杂性，要把状态和转移显式建模

---

## 23. 一句话原则

> `uc-app` 的职责不是“写几个能跑的调用链”，而是“把 `uc-core` 的能力组织成清晰、稳定、可观测、可测试的应用流程”。
