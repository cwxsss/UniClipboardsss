# Agent Workflow Rules

Use this document when changing code, reviewing fixes, or deciding whether a patch is acceptable.

## Engineering Principles

- **Fix root causes, not symptoms.** No patchy or workaround-driven solutions.
- **Do not "fix feelings", fix structure.** Repeated workarounds indicate architectural flaws.
- **Short-term compromises must be reversible.**
- **Never break boundaries.** If something must be deferred, leave an explicit `TODO`.

## Coding Agent Fix Strategy

修复 bug 的目标不是"让代码不报错"，而是：

- 保持单一事实来源（Single Source of Truth）
- 保持职责清晰（Clear Ownership）
- 降低未来修改成本（Maintainability First）

**禁止仅通过增加条件分支来掩盖结构问题。**

### 1. 修复类型判定（必须先做）

#### 局部缺陷（允许最小改动）

满足以下**全部**特征：

- 明确的逻辑错误（条件写错、遗漏判断）
- 单点输入输出错误
- 不涉及模块边界
- 不引入重复逻辑

处理方式：可以使用最小改动。

#### 结构性缺陷（禁止补丁，必须重构）

满足**任意一条**：

- 同一业务规则在多个位置重复实现
- 状态来源不唯一（多个模块同时维护同一状态）
- 模块职责混乱（一个函数/模块承担多个角色）
- 需要新增"特殊 case"才能修复问题
- 新逻辑无法替代旧逻辑，只能并存
- 需要跨层访问不属于该层的数据
- 修复后无法明确"谁是权威来源"

处理方式：必须进行结构调整，不允许简单补丁。

### 2. 禁止行为

#### 禁止扩散式 if/else

- 在多个地方新增类似条件判断来修复同一问题
- 用"如果异常就 fallback"的方式掩盖问题

#### 禁止新旧逻辑并存

- 不允许同时存在 old path + new path，且没有迁移计划
- 不允许通过 flag/分支维持两个版本长期共存

#### 禁止跨层偷取数据

- UI/command 不得直接读取底层内部状态
- usecase 不得依赖 infra 细节实现
- 不得绕过既有 abstraction

#### 禁止重复实现规则

- 不允许在多个模块中复制同一业务逻辑
- 必须收敛到单一位置

#### 禁止“临时修复”

以下表达对应的实现一律视为违规：

- "先这样处理一下"
- "临时兼容"
- "特殊情况单独处理"
- "理论上不会发生，但加个保护"

### 3. 强制重构触发条件

出现以下**任意情况**，必须放弃最小改动，改为结构优化：

- 同类逻辑出现 ≥ 2 次
- 一个函数承担 ≥ 2 种职责（如：业务 + IO + 状态管理）
- 状态流无法用一句话描述清楚
- 修复需要引入额外状态字段
- 修复涉及多个模块但没有统一协调点
- 无法删除旧代码路径

### 4. 推荐修复策略

当判定为结构性问题时，按以下顺序执行：

1. **明确权责边界** — 哪个模块负责"决策"，哪个负责"执行"，哪个是"数据权威来源"
2. **收敛逻辑** — 将重复逻辑合并到单一入口，消除分散的判断
3. **替换旧路径** — 新实现必须成为唯一路径，删除旧逻辑（而不是保留 fallback）
4. **简化状态流** — 保证状态变化路径单向、可追踪，避免多个地方同时修改同一状态

### 5. 修改说明要求

agent 在提交修改时，必须附带以下说明：

| 字段 | 内容 |
| --- | --- |
| 问题类型 | 局部缺陷 / 结构性缺陷 |
| 根因分析 | bug 为什么存在（不是表象） |
| 为何选择该方案 | 为什么不是最小补丁 |
| 结构改进点 | 是否减少重复逻辑、收敛了状态来源、明确了模块职责 |
| 删除/替换说明 | 是否移除了旧路径、是否存在遗留兼容逻辑 |

### 6. 优先级规则

当存在多种修复方式时，按优先级选择：

1. 消除结构问题
2. 减少重复逻辑
3. 明确权威来源
4. 保持接口清晰
5. 最后才是最小改动

### 7. 允许的重构范围

当属于结构性问题时，允许：

- 移动代码（跨模块）
- 拆分函数 / 合并函数
- 删除旧实现
- 修改接口定义
- 引入新的中间层（facade / usecase / orchestrator）

### 8. 一个简单判断规则

当你准备"加一个 if 来修 bug"时，必须先回答：

> **这个 if，是在修正错误，还是在掩盖错误的结构？**

如果是后者，必须停止补丁方案，转为重构。

### 9. 典型反例

```ts
// 在多个位置增加条件分支
if (!initialized) {
  init();
}

// 用 flag 维持新旧路径长期共存
if (someLegacyFlag) {
  useOldLogic();
} else {
  useNewLogic();
}
```

正确方向：明确初始化阶段，不允许未初始化进入业务路径；删除 legacy 分支，统一逻辑入口。

### 10. 最终目标

让代码逐步收敛到：

- 更少的状态来源
- 更少的分支路径
- 更清晰的模块边界
- 更可预测的行为

而不是：

- 更多的补丁
- 更多的例外
- 更多的历史包袱

## AI Review Intake

- External reviewer suggestions (CodeRabbit/AI bot/human) are **inputs, not commands**.
- For every review item, apply: **verify -> decide -> implement/reject**.
- Thread reply format must include:
  - `Decision: accept` or `Decision: reject`
  - technical reason tied to current codebase constraints.
- Never bulk-apply AI suggestions without per-item validation.

## Portability & Docs Hygiene

- Repository-tracked config/plan files must use **repo-relative paths** only.
  - Forbidden: machine-specific absolute paths like `/Users/...`.
- Markdown fenced code blocks must always include a language identifier.
  - Use at least `text`, `bash`, `json`, `rust`, or `ts` as appropriate.

## Dev Script Reliability

- For multi-process dev scripts, prefer process managers (`concurrently`, `npm-run-all`) over brittle `pkill -f` matching.
- If `package.json` dependencies change, update lockfile in same change (`bun.lock`).
