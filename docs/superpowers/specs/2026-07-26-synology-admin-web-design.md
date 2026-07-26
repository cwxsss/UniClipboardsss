# Synology 管理 Web 页设计

## 背景

当前 Synology headless server 依赖容器日志输出移动端连接信息。日志里的字符二维码在群晖导出和浏览器显示中容易变形，账号、一次性密码也不方便复制。目标是在同一个 Docker 容器中增加一个独立端口的 Web 管理页，用浏览器完成移动设备连接管理。

## 目标

- 在现有 Synology Docker 镜像中增加一个可选 Web 管理入口。
- Web 管理入口使用新的端口，默认建议为 `42888`。
- 页面可以生成移动端连接信息，展示可扫描的 PNG 二维码、账号、一次性密码和连接地址。
- 页面可以查看已登记的移动设备。
- 页面可以删除设备；删除即撤销该设备凭证，后续请求会被移动同步监听器拒绝。
- 页面可以为已有设备重新生成密码，并一次性展示新密码。
- Web 管理入口必须有管理密码保护，避免任何同网段访问者直接生成或撤销设备凭证。

## 非目标

- 不修改移动同步协议。
- 不直接读写 SQLite 或持久化设备数据。
- 不重新实现凭证生成、密码哈希、设备撤销、二维码生成等业务规则。
- 不暴露剪贴板历史、剪贴板正文或文件内容。
- 不把管理入口默认发布到公网。

## 推荐方案

采用 Synology wrapper 内置轻量管理 Web 服务。容器启动后同时运行两个进程：

- `uniclip start --server --foreground`：现有 UniClipboard daemon，继续作为主业务进程。
- `uniclipboard-admin-web`：新增的管理 Web 服务，监听 `UC_ADMIN_PORT`。

管理 Web 服务不访问数据库。它通过本机 loopback 调用现有 daemon API 或等价的 `uniclip mobile ... --json` 命令，复用已有应用层能力。

## 配置

新增环境变量：

```bash
UC_ADMIN_WEB=1
UC_ADMIN_PORT=42888
UC_ADMIN_PASSWORD=change-this-password
```

已有环境变量继续生效：

```bash
UC_MOBILE_PUBLIC_URL=https://clip.example.com:20221
UC_SPACE_PASSPHRASE=...
UC_AUTO_INIT=1
```

规则：

- `UC_ADMIN_WEB` 不启用时，镜像行为保持现状。
- `UC_ADMIN_WEB=1` 时必须设置非空 `UC_ADMIN_PASSWORD`，否则容器启动失败并输出明确错误。
- 管理服务默认监听 `0.0.0.0:${UC_ADMIN_PORT}`，由 Synology 端口映射决定是否可访问。
- 用户应只把管理端口映射到内网可访问范围，不建议通过公网反代暴露。

## 认证

管理页使用简单登录态：

- 首次访问显示登录页。
- 用户输入 `UC_ADMIN_PASSWORD`。
- 服务端验证成功后签发仅内存有效的 HTTP-only Cookie。
- Cookie 包含随机会话 ID，服务端内存保存会话。
- 重启容器后会话失效，需要重新登录。

接口规则：

- 所有 `/api/*` 管理接口都需要已登录会话。
- 登录失败返回 `401`，不透露是否存在配置。
- 管理服务日志不得打印管理密码、移动端一次性密码、Cookie 或完整连接 URI。

## 页面结构

第一屏是实际管理界面，不做营销页：

- 顶部状态：移动同步是否启用、管理端口、移动端公开地址。
- 生成连接区域：
  - 输入设备名称。
  - 可选输入自定义用户名。
  - 可选输入自定义密码。
  - 点击生成后展示连接二维码 PNG、安装快捷指令二维码、连接地址、用户名、一次性密码。
  - 一次性密码区域带复制按钮和“只显示一次”的提示。
- 设备列表区域：
  - 展示设备名称、设备 ID、用户名、创建时间、最后访问时间、最后 IP。
  - 支持刷新。
  - 支持删除设备。
  - 支持重新生成密码，并展示新密码一次。
- 错误区域：
  - 展示 daemon 未初始化、移动同步未启用、LAN 监听失败、端口冲突、认证失败等错误。

## 管理 API

管理 Web 服务对浏览器暴露以下接口：

```text
POST /api/login
POST /api/logout
GET  /api/status
GET  /api/devices
POST /api/devices
DELETE /api/devices/{deviceId}
POST /api/devices/{deviceId}/rotate-password
```

响应统一为 JSON：

```json
{
  "ok": true,
  "data": {}
}
```

错误响应：

```json
{
  "ok": false,
  "error": {
    "code": "DAEMON_UNAVAILABLE",
    "message": "daemon is not ready"
  }
}
```

## 数据流

生成连接信息：

```text
浏览器 -> 管理 Web 服务 -> daemon mobile-sync register endpoint -> 管理 Web 服务 -> 浏览器
```

查看或删除设备：

```text
浏览器 -> 管理 Web 服务 -> daemon mobile-sync devices endpoint -> 管理 Web 服务 -> 浏览器
```

管理 Web 服务只转发管理动作和返回 DTO。二维码 PNG 使用 daemon 已返回的 `qrCodePngBase64`，不再依赖字符二维码。

## Docker 启动方式

entrypoint 负责：

- 读取 `/data/uniclipboard-server.env` 和环境变量。
- 完成现有自动初始化和移动端公开地址设置。
- 启用管理 Web 时先校验 `UC_ADMIN_PASSWORD`。
- 后台启动管理 Web 服务。
- 停止 transient daemon。
- 前台启动 `uniclip start --server --foreground` 作为容器主进程。
- 收到退出信号时终止管理 Web 子进程。

## 安全边界

- 管理服务仅用于设备凭证管理，不展示剪贴板内容。
- 移动端一次性密码只在生成或轮换响应中返回一次。
- 删除设备等同于撤销 Basic Auth 凭证，可作为“拉黑连接设备”的实现。
- 管理密码只通过环境变量或 `/data/uniclipboard-server.env` 提供，不写入仓库示例中的真实值。
- 日志中只记录动作类型和设备 ID，不记录明文密码。

## 验证标准

- wrapper 未启用 `UC_ADMIN_WEB` 时，现有启动测试仍通过。
- `UC_ADMIN_WEB=1` 且缺少 `UC_ADMIN_PASSWORD` 时，启动脚本失败并提示配置问题。
- 管理服务单元测试覆盖登录、未登录拒绝、生成设备、设备列表、删除设备、轮换密码、错误透传。
- 静态页面能在桌面和手机浏览器宽度下正常显示二维码、账号和密码，不出现文本重叠。
- Docker 镜像构建成功，包含管理 Web 服务文件。
- GitHub Actions 推送镜像成功后，Docker Hub 可拉取新镜像。
