# 📘 uc-core / AGENTS.md

## 1. 文档目的

`uc-core` 是 **UniClipboard** 的领域核心，承载系统最稳定、最持久的业务语义。本规范用于指导所有开发者和 AI Agent 在修改 `uc-core` 代码时，确保：

* 领域边界清晰
* 架构长期可维护
* 避免技术实现污染领域模型
* 保持跨平台与实现无关
* 支持未来的扩展（桌面、移动端、CLI、云端）

**任何对 `uc-core` 的修改，都必须遵循本规范并进行自我审查。**

---

## 2. uc-core 的定位

### 2.1 核心职责

`uc-core` 只负责以下内容：

1. **领域模型（Domain Model）**

   * 实体（Entities）
   * 值对象（Value Objects）
   * 枚举（Enums）
   * 领域错误（Domain Errors）

2. **领域规则（Business Rules）**

   * 状态转换
   * 业务约束
   * 不依赖运行环境的逻辑

3. **领域事件（Domain Events）**

4. **端口抽象（Ports）**

   * 为应用层提供与外部系统交互的能力抽象

5. **领域策略（Policies）**

   * 如安全策略、同步策略、保留策略等

---

### 2.2 非职责（禁止进入 uc-core）

以下内容**严禁**出现在 `uc-core` 中：

| 类别      | 示例                                   |
| ------- | ------------------------------------ |
| 平台相关    | OS 路径、环境变量、AppData、Keychain          |
| 网络实现    | libp2p、HTTP、WebSocket、TCP            |
| 数据库     | SQLite、Diesel、SQL 语句                 |
| 文件系统    | 具体文件读写实现                             |
| 加密算法实现  | Argon2、AES、XChaCha20 等库调用            |
| UI 相关   | Tauri、前端 DTO                         |
| 应用流程    | Orchestrator、UseCase、Command Handler |
| 启动逻辑    | Wiring、Bootstrap                     |
| API 协议  | REST/IPC 字段、序列化格式                    |
| 第三方 SDK | 任何具体实现依赖                             |

---

## 3. 分层架构关系

```text
            +----------------------+
            |        uc-app        |
            |  (UseCases / Facade) |
            +----------▲-----------+
                       |
                       |  Ports
                       |
            +----------▼-----------+
            |        uc-core       |
            |   (Domain Model)     |
            +----------▲-----------+
                       |
                       | Implementations
                       |
            +----------▼-----------+
            |       uc-infra       |
            | (DB / Network / FS)  |
            +----------------------+
```

`uc-core` 只依赖标准库，不依赖任何具体实现。

---

## 4. 领域建模原则

### 4.1 实体（Entities）

* 具有唯一标识（ID）
* 生命周期可追踪
* 包含业务行为而非仅数据

**示例：**

```rust
pub struct ClipboardEntry {
    pub id: EntryId,
    pub device_id: DeviceId,
    pub created_at: Timestamp,
    pub content: ClipboardContent,
}
```

---

### 4.2 值对象（Value Objects）

* 不可变
* 通过值判断相等性
* 无独立生命周期

**示例：**

```rust
pub struct DeviceName(String);
```

---

### 4.3 领域服务（Domain Services）

用于表达不适合放入单一实体的业务逻辑。

```rust
pub struct ClipboardDeduplicationService;

impl ClipboardDeduplicationService {
    pub fn is_duplicate(a: &ClipboardEntry, b: &ClipboardEntry) -> bool {
        a.content_hash == b.content_hash
    }
}
```

---

### 4.4 领域事件（Domain Events）

用于表达业务状态变化，而非技术事件。

```rust
pub enum DomainEvent {
    ClipboardEntryCaptured { entry_id: EntryId },
    DevicePaired { device_id: DeviceId },
    SpaceUnlocked { space_id: SpaceId },
}
```

---

## 5. Ports 设计规范

### 5.1 Ports 的作用

Ports 用于定义领域所需的外部能力，而不是具体实现。

### 5.2 Ports 设计原则

| 原则      | 说明                       |
| ------- | ------------------------ |
| 以业务能力命名 | 如 `DeviceRepositoryPort` |
| 不暴露技术细节 | 不出现 HTTP、libp2p 等        |
| 面向领域对象  | 使用 `DeviceId` 等          |
| 保持最小接口  | 避免过度设计                   |

### 5.3 示例

```rust
#[async_trait]
pub trait DeviceRepositoryPort: Send + Sync {
    async fn get_by_id(&self, id: &DeviceId) -> Result<Option<Device>, DeviceError>;
    async fn save(&self, device: &Device) -> Result<(), DeviceError>;
}
```

---

## 6. Network 相关建模原则

### 6.6 核心思想

> **uc-core 关注的是设备之间的“关系”，而不是“通信方式”。**

### 6.2 可以存在于 core 的内容

* `PairedDevice`
* `ConnectionPolicy`
* `DeviceAddress`（逻辑地址）
* 领域事件（如设备上线）

### 6.3 不应存在于 core 的内容

| 不允许             | 原因        |
| --------------- | --------- |
| libp2p protocol | 技术实现      |
| protocol IDs    | 与具体网络协议绑定 |
| HTTP/WebSocket  | 传输层细节     |
| API 字符串         | 表示层细节     |
| 序列化结构           | 技术实现      |

---

## 7. Crypto 领域建模原则

### 7.1 可以存在于 core 的内容

* `MasterKey`
* `KeySlot`
* `WrappedKey`
* `Passphrase`
* `UnlockContext`
* `EncryptionPolicy`

### 7.2 不允许存在于 core 的内容

| 不允许         | 示例                  |
| ----------- | ------------------- |
| 加密算法调用      | `argon2::hash`      |
| 随机数实现       | `rand::rngs::OsRng` |
| Keychain 访问 | OS API              |
| Nonce 生成    | 技术实现                |

---

## 8. Settings 与 Config 的边界

| 类型   | 是否属于 core | 示例             |
| ---- | --------- | -------------- |
| 业务设置 | ✅         | `SyncSettings` |
| 配置加载 | ❌         | 读取 TOML        |
| 环境变量 | ❌         | `std::env`     |
| 默认路径 | ❌         | `AppData`      |

---

## 9. Setup 与 Orchestration

### 9.1 不属于 core

以下内容应放在 `uc-app`：

* Pairing 状态机
* Setup 流程
* UseCase 编排
* 用户交互流程

### 9.2 core 中允许的内容

* `Space`
* `TrustRelationship`
* `Device`
* `KeyMaterial`

---

## 10. 依赖管理规则

### 10.1 允许的依赖

* Rust 标准库
* 轻量级工具库（如 `thiserror`, `serde` 用于领域建模）

### 10.2 禁止的依赖

| 禁止    | 示例                  |
| ----- | ------------------- |
| 网络库   | `libp2p`, `reqwest` |
| 数据库   | `diesel`, `sqlx`    |
| UI    | `tauri`             |
| 异步运行时 | `tokio`             |
| 加密实现  | `ring`, `argon2`    |

---

## 11. 命名规范

| 类型  | 命名规则      | 示例                   |
| --- | --------- | -------------------- |
| 实体  | 名词        | `Device`             |
| 值对象 | 名词        | `DeviceId`           |
| 端口  | `*Port`   | `BlobRepositoryPort` |
| 错误  | `*Error`  | `DeviceError`        |
| 事件  | 过去式       | `DevicePaired`       |
| 策略  | `*Policy` | `RetentionPolicy`    |

---

## 12. 代码修改自我审查清单（必须执行）

在提交任何涉及 `uc-core` 的变更前，开发者必须逐项确认：

### 12.1 边界检查

* [ ] 该修改是否引入了平台或基础设施依赖？
* [ ] 是否包含 HTTP、数据库、文件系统或网络实现细节？
* [ ] 是否引入了 UI 或 API 相关概念？
* [ ] 是否依赖具体加密算法实现？

### 12.2 领域合理性

* [ ] 该概念在脱离当前运行环境后仍然成立吗？
* [ ] 是否体现真实的业务语义？
* [ ] 是否属于领域规则而非流程编排？

### 12.3 Ports 设计

* [ ] 是否以业务能力为导向？
* [ ] 是否避免技术细节泄漏？
* [ ] 是否保持接口最小化？

### 12.4 依赖检查

* [ ] 是否仅依赖允许的库？
* [ ] 是否避免引入 `tokio`、`libp2p` 等实现？

---

## 13. 示例：正确与错误对比

### ❌ 错误示例

```rust
use libp2p::PeerId; // 不允许

pub struct NetworkDevice {
    pub peer_id: PeerId,
}
```

### ✅ 正确示例

```rust
pub struct DeviceId(String);

pub struct Device {
    pub id: DeviceId,
}
```

---

## 14. 提交规范

* 所有涉及 `uc-core` 的提交必须在 PR 描述中说明：

  * 修改的领域概念
  * 是否影响领域边界
  * 自我审查清单的确认

**PR 模板示例：**

```text
### uc-core Change Summary

- [ ] 修改仅涉及领域模型
- [ ] 未引入基础设施依赖
- [ ] Ports 设计符合规范
- [ ] 已完成自我审查
```

---

## 15. 评审原则

Code Review 时应重点关注：

1. 是否存在技术细节泄漏
2. 是否破坏领域边界
3. 是否引入不必要的抽象
4. 是否影响跨平台能力
5. 是否符合统一语言（Ubiquitous Language）

---

## 16. 总结

### uc-core 的核心原则

> **Stable · Pure · Business-Oriented · Implementation-Agnostic**

| 原则                      | 含义      |
| ----------------------- | ------- |
| Stable                  | 变化频率最低  |
| Pure                    | 不包含技术实现 |
| Business-Oriented       | 只表达业务语义 |
| Implementation-Agnostic | 与平台无关   |
