# UniClipboard 群晖 Headless 节点

这个镜像运行的是**持久在线 Space 成员**：它加入一个 UniClipboard Space，接收和发送桌面端的 iroh 同步事件，并提供移动端 `mobile-sync` 网关和内网管理页面。

它不是 iroh relay，也不保存其他设备离线时等待转发的剪贴板。需要自建 relay 时，使用仓库中的 [`deploy/relay/`](../relay/README_ZH.md)；两种服务必须分开部署。

## 镜像与数据目录

```text
chuais/uniclipboard-server:latest
```

将群晖的持久化目录映射到容器 `/data`，例如：

```text
/volume1/docker/uniclipboard/data -> /data
```

`/data` 包含节点身份、Space 成员资格、密钥材料、加密数据库和移动端凭据。备份它；删除它会让节点成为一个全新的设备。

## 端口

| 容器端口 | 建议映射 | 用途 |
| --- | --- | --- |
| `42720/tcp` | `20221 -> 42720`，或只给 Nginx 反向代理 | 移动端 `mobile-sync` 网关 |
| `42888/tcp` | `42888 -> 42888`，仅内网 | 管理页面 |

若为这个节点配置固定 iroh 直连端口，另行映射和放行对应的 **UDP** 端口。它不是 relay 的 `7842/udp`。

## 首次启动：二选一

首次启动时，入口脚本只接受 `UC_SPACE_BOOTSTRAP_MODE` 的两种值：`init` 或 `join`。容器检查到 `/data` 已完成设置后，不会再次创建或加入 Space，因此重启时不需要保留邀请码和 Space 口令。

不要配置旧的自动初始化变量；它们不会触发任何自动操作。

### 路径一：创建新 Space

适用于你想让群晖节点创建一个全新的 Space，再从管理页或已有客户端邀请其他设备。

```text
HOME=/data
UC_SPACE_BOOTSTRAP_MODE=init
UC_SPACE_PASSPHRASE=自行设置的强口令
UC_DEVICE_NAME=Synology Server
UC_MOBILE_PUBLIC_URL=https://clip.example.com:20221
UC_ADMIN_WEB=1
UC_ADMIN_PORT=42888
UC_ADMIN_PASSWORD=自行设置的管理页密码
```

首次启动会执行：

```bash
uniclip init --passphrase "$UC_SPACE_PASSPHRASE" --device-name "$UC_DEVICE_NAME"
```

### 路径二：加入已有 Space

适用于已有一台桌面端已经在目标 Space 中。先在那台已加入的桌面端生成邀请码，然后把邀请码和**同一个 Space 口令**写入群晖环境变量或 `/data/uniclipboard-server.env`。

```text
HOME=/data
UC_SPACE_BOOTSTRAP_MODE=join
UC_SPACE_INVITE_CODE=从已有成员生成的邀请码
UC_SPACE_PASSPHRASE=已有 Space 的口令
UC_DEVICE_NAME=Synology Server
UC_MOBILE_PUBLIC_URL=https://clip.example.com:20221
UC_ADMIN_WEB=1
UC_ADMIN_PORT=42888
UC_ADMIN_PASSWORD=自行设置的管理页密码
```

首次启动会执行：

```bash
uniclip join --code "$UC_SPACE_INVITE_CODE" --passphrase "$UC_SPACE_PASSPHRASE" --device-name "$UC_DEVICE_NAME"
```

这条路径不需要进入容器，也不需要在容器运行后手动执行 `join`。邀请码和口令仅用于首次置备；成功后应从群晖环境变量或配置文件中移除，避免长期暴露。

## 使用配置文件

群晖图形界面中的环境变量与配置文件二选一。配置文件默认路径为：

```text
/data/uniclipboard-server.env
```

每行必须是 `KEY=value`。包含空格的值请用引号，例如：

```sh
UC_DEVICE_NAME="Synology Server"
```

可通过 `UC_SERVER_CONFIG` 指定另一个配置文件路径。配置文件在容器启动时加载，适合不希望每次编辑容器配置时重填环境变量的情况。

## 移动端和管理页面

`UC_MOBILE_PUBLIC_URL` 是写入移动端连接信息的公网地址，例如：

```text
UC_MOBILE_PUBLIC_URL=https://clip.example.com:20221
```

它应指向 Nginx 反向代理后的移动端网关，而不是 iroh relay。Nginx 上游指向群晖宿主机映射的 `42720/tcp`；管理页面 `42888/tcp` 不应公开到互联网。

启动后在内网访问：

```text
http://群晖内网IP:42888
```

管理页面用于创建移动端凭据和桌面端邀请码。手机二维码只适用于 `mobile-sync` 客户端，桌面端和支持 Space 的鸿蒙客户端使用 Space 邀请码与口令。

## 可选网络变量

```text
UC_IROH_BIND_PORT=42999
UC_IROH_PUBLIC_ADDR=203.0.113.10:42999
```

这两个变量只用于让这个**节点**公布固定的 iroh UDP 直连地址。`UC_IROH_PUBLIC_ADDR` 当前只接受 `IPv4:端口` 或 `IPv6:端口`，不接受域名。它们不是自建 relay 的配置，未设置时 iroh 仍可使用默认的 NAT 穿透和 relay fallback。

## 常见问题

### `setup is incomplete`

检查 `/data` 是否为空，以及首次启动是否填写了以下任一完整配置：

```text
UC_SPACE_BOOTSTRAP_MODE=init
UC_SPACE_PASSPHRASE=...
```

或：

```text
UC_SPACE_BOOTSTRAP_MODE=join
UC_SPACE_INVITE_CODE=...
UC_SPACE_PASSPHRASE=...
```

不要在已存在的 `/data` 上尝试用另一套 Space 凭据覆盖加入。要切换 Space，应先备份并删除该节点的数据目录，再按 `join` 路径重新置备。

### 移动端无法连接

确认手机可访问 `UC_MOBILE_PUBLIC_URL`，并检查 Nginx 是否将请求转发到容器的 `42720`。从公网访问 `/SyncClipboard.json` 返回 `401` 表示网关可达但需要移动端凭据，这是正常状态。

### 跨网络桌面端仍无法同步

确认桌面端未开启 LAN-only 模式。需要使用私有 iroh relay 时，部署 [`deploy/relay/`](../relay/README_ZH.md)，再在每台支持自定义 relay 的客户端中填入 relay URL；只部署 relay 不会自动把 URL 下发给所有客户端。
