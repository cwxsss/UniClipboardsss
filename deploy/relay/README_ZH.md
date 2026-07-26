# 独立部署 iroh relay

这个目录部署的是 iroh relay，不是 UniClipboard headless 节点。relay 仅在两台设备不能直连时转发加密的 iroh 流量：它不加入 Space、不需要 Space 口令或邀请码、不提供移动端 HTTP 网关，也不保存剪贴板历史。

群晖 headless 节点请使用 [`deploy/synology/`](../synology/README_ZH.md)。两个服务不要合并到同一个 Compose 或同一套数据卷。

## 版本策略

iroh 的 relay 协议需要与客户端使用的 iroh 版本匹配。本仓库当前使用 `iroh 1.0.0-rc.1`。推荐使用由 GitHub Actions 发布的 `chuais/iroh-relay:1.0.0-rc.1`，避免在部署 VPS 上下载 Rust 依赖和编译源码。

更新 UniClipboard 的 iroh 依赖时，必须同步更新 `IROH_RELAY_VERSION`，并重新构建 relay：

```bash
IROH_RELAY_IMAGE=chuais/iroh-relay:1.0.0-rc.1 docker compose pull
IROH_RELAY_IMAGE=chuais/iroh-relay:1.0.0-rc.1 docker compose up -d --no-build
```

默认 Compose 保留本地源码构建能力。通过 `IROH_RELAY_IMAGE` 设置镜像名并使用 `--no-build`，即可仅拉取预构建镜像。

## 前置条件

- 一台具有公网 IPv4 的 Linux 主机或 VPS。
- 一个解析到该主机公网地址的域名，例如 `relay.example.com`。
- 防火墙和云安全组放行 `80/tcp`、`443/tcp`、`7842/udp`。
- 80/443 由 iroh relay 直接占用最简单；若已被 Nginx 或 Caddy 占用，必须正确转发 `/relay`、`/ping`、`/generate_204` 和 `/`，同时仍要单独放行 UDP 7842。建议将 relay 放在独立 VPS 或独立公网 IP。

## 部署

```bash
cd deploy/relay
cp config.toml.example config.toml
```

编辑 `config.toml`：

```toml
[tls]
hostname = "relay.example.com"
contact = "admin@example.com"
```

域名的 A/AAAA 记录必须在启动前解析到此服务器。然后构建并启动：

```bash
IROH_RELAY_IMAGE=chuais/iroh-relay:1.0.0-rc.1 docker compose pull
IROH_RELAY_IMAGE=chuais/iroh-relay:1.0.0-rc.1 docker compose up -d --no-build
docker compose logs -f relay
```

首次启动会由 iroh relay 向 Let's Encrypt 申请证书。证书状态保存在命名卷 `relay-certs`，不要随意删除。

## 验证

```bash
curl -sS -o /dev/null -w 'http_code=%{http_code}\n' https://relay.example.com/ping
```

返回 `http_code=200` 表示 HTTPS relay 可达。还应确认宿主机防火墙放行了 `7842/udp`，它用于 QUIC 地址发现和 NAT 探测。

## 在客户端启用

在**每台支持自定义 relay 的 UniClipboard 桌面端**中打开“设置 → 网络 → 自定义中继节点”，填入：

```text
https://relay.example.com
```

配置为空时客户端使用 iroh 默认 relay；填入后客户端只使用这里配置的 relay。该设置存储在每台客户端本地，部署 relay 不会自动下发地址。

旧版或没有“自定义中继节点”设置的鸿蒙客户端不能仅靠部署 relay 获得支持，需要客户端自身加入该配置能力。relay 也不会修复客户端已加入 Space 但没有处理收到剪贴板事件的问题。

## 与 headless 节点的关系

| 服务 | 是否加入 Space | 是否保存节点状态 | 是否需要 Space 凭据 | 对外端口 |
| --- | --- | --- | --- | --- |
| 群晖 headless 节点 | 是 | 是，`/data` | 首次置备时需要 | `42720/tcp`、可选固定 iroh UDP |
| iroh relay | 否 | 仅 TLS 证书 | 否 | `80/tcp`、`443/tcp`、`7842/udp` |

当设备可以直接建立 iroh 连接时，relay 不在数据路径中；只有打洞失败时才作为加密传输的后备通道。
