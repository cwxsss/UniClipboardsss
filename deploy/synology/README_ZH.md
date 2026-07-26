# UniClipboard 群晖 Docker 部署说明

本文档说明当前 Docker 镜像的用途、端口、环境变量、群晖图形化部署方式，以及手机端和桌面端如何接入。

## 项目说明

UniClipboard 是一个跨设备剪贴板同步工具。桌面端之间使用 Space 成员关系和 iroh 网络同步；手机端当前通过 `mobile-sync` HTTP 网关兼容 SyncClipboard 协议。

本目录维护的是面向群晖的封装镜像，不是重写 UniClipboard 主程序。它在官方 headless server 镜像基础上增加了：

- 容器启动时自动初始化 Space。
- 容器启动时自动配置手机同步公网地址。
- 同一个容器内启动一个轻量 Web 管理页。
- Web 管理页生成手机端二维码、账号和一次性密码。
- Web 管理页查看、删除手机端设备，重置手机端密码。
- Web 管理页生成桌面端 Space 邀请码和 `uniclip join` 命令。

## 镜像

推荐使用：

```text
chuais/uniclipboard-server:latest
```

也可以固定当前版本：

```text
chuais/uniclipboard-server:0.20.0-alpha.2
```

该镜像支持：

- `linux/amd64`
- `linux/arm64`

## 数据目录

容器内数据目录是：

```text
/data
```

群晖上建议映射到一个持久目录，例如：

```text
/volume1/docker/uniclipboard/data -> /data
```

这个目录会保存 Space 身份、数据库、密钥材料和移动端设备凭据。删除该目录等于重建一个新的 UniClipboard 节点。

## 端口

| 容器端口 | 建议映射 | 用途 |
| --- | --- | --- |
| `42720/tcp` | `20221 -> 42720` 或只给 Nginx 内网反代 | 手机端 `mobile-sync` 网关 |
| `42888/tcp` | `42888 -> 42888` | Web 管理页 |

管理页端口只建议内网访问，不建议暴露到公网。

手机端公网访问建议用 Nginx / 群晖反向代理把域名转发到容器的 `42720`，例如：

```text
https://clip.example.com:20221 -> http://群晖内网IP:20221
```

如果你直接把群晖宿主机的 `20221` 映射到容器 `42720`，反代上游就是：

```text
http://127.0.0.1:20221
```

## 必填环境变量

最小可用配置：

```text
HOME=/data
UC_AUTO_INIT=1
UC_SPACE_PASSPHRASE=换成你的空间口令
UC_DEVICE_NAME=Synology Server
UC_MOBILE_PUBLIC_URL=https://你的域名:端口
UC_ADMIN_WEB=1
UC_ADMIN_PORT=42888
UC_ADMIN_PASSWORD=换成你的管理页密码
```

说明：

- `HOME=/data`：让 UniClipboard 把配置和数据库写入持久目录。
- `UC_AUTO_INIT=1`：如果 `/data` 里还没有 Space，就自动创建。
- `UC_SPACE_PASSPHRASE`：Space 空间口令，桌面端加入时也需要用到。
- `UC_DEVICE_NAME`：这个群晖 server 节点在 Space 里的设备名。
- `UC_MOBILE_PUBLIC_URL`：手机端二维码里显示的访问地址，通常填你的域名和端口。
- `UC_ADMIN_WEB=1`：启用 Web 管理页。
- `UC_ADMIN_PORT=42888`：Web 管理页监听端口。
- `UC_ADMIN_PASSWORD`：Web 管理页登录密码。

## 可选环境变量

```text
UC_MOBILE_LABEL=meta60
UC_ADMIN_SHOW_SPACE_PASSPHRASE=0
UC_SERVER_CONFIG=/data/uniclipboard-server.env
```

说明：

- `UC_MOBILE_LABEL`：容器第一次启动时自动创建一个手机设备，值就是设备显示名。现在已经有 Web 管理页，一般可以不填。
- `UC_ADMIN_SHOW_SPACE_PASSPHRASE=1`：Web 管理页生成桌面端连接命令时，是否把 Space 口令直接写进命令。默认不显示，安全一些。
- `UC_SERVER_CONFIG`：可选配置文件路径。默认会读取 `/data/uniclipboard-server.env`。群晖环境变量和这个文件二选一即可；如果两边都写，实际效果取决于启动脚本读取后的变量值。

## 群晖图形化部署

1. 打开 Container Manager。
2. 注册表里搜索或手动拉取镜像：

```text
chuais/uniclipboard-server:latest
```

3. 创建容器。
4. 卷映射：

```text
/volume1/docker/uniclipboard/data -> /data
```

5. 端口映射：

```text
20221 -> 42720
42888 -> 42888
```

6. 环境变量填入最小可用配置。
7. 启动容器。
8. 打开管理页：

```text
http://群晖内网IP:42888
```

## 手机端连接

1. 打开 Web 管理页。
2. 登录管理密码。
3. 在“生成连接信息”里输入手机名称。
4. 点击生成二维码和账号密码。
5. 手机扫描“连接二维码”。

生成的手机端连接信息只适用于 mobile-sync 客户端。它包含：

```text
服务器 URL
mobile 用户名
一次性密码
```

手机实际访问的是：

```text
https://你的域名:端口/SyncClipboard.json
```

## 桌面端连接

桌面端不能使用手机端二维码。桌面端需要加入同一个 Space。

操作方式：

1. 打开 Web 管理页。
2. 找到“桌面端连接”。
3. 点击“生成桌面端邀请”。
4. 在新桌面设备上执行页面显示的命令：

```bash
uniclip join --code <邀请码> --passphrase <你的空间口令>
```

如果设置：

```text
UC_ADMIN_SHOW_SPACE_PASSPHRASE=1
```

页面会把 `UC_SPACE_PASSPHRASE` 写进完整命令。否则页面只显示命令模板，你需要手动输入 Space 口令。

注意：同一时间只保留一个桌面端邀请。生成新邀请会替换旧邀请。

## Nginx 反向代理建议

只反代手机端网关，不要把管理页暴露公网。

手机端反代目标：

```text
http://群晖内网IP:20221
```

外部访问地址填入：

```text
UC_MOBILE_PUBLIC_URL=https://你的域名:端口
```

如果你的外部地址是标准 HTTPS 443 端口，可以不写端口：

```text
UC_MOBILE_PUBLIC_URL=https://clip.example.com
```

如果你使用非标准端口，例如 `20221`，必须写端口：

```text
UC_MOBILE_PUBLIC_URL=https://clip.example.com:20221
```

## 配置文件方式

除了群晖图形界面环境变量，也可以把配置写入：

```text
/data/uniclipboard-server.env
```

示例：

```sh
HOME=/data
UC_AUTO_INIT=1
UC_SPACE_PASSPHRASE=replace-with-your-space-passphrase
UC_DEVICE_NAME=Synology Server
UC_MOBILE_PUBLIC_URL=https://clip.example.com:20221
UC_ADMIN_WEB=1
UC_ADMIN_PORT=42888
UC_ADMIN_PASSWORD=replace-with-your-admin-password
UC_ADMIN_SHOW_SPACE_PASSPHRASE=0
```

注意：`.env` 文件每一行都是 `KEY=value`，不要写成说明文字，也不要写中文冒号。

## 常见问题

### 报错 `setup not complete`

通常是 `/data` 里没有初始化成功，或没有正确设置：

```text
HOME=/data
UC_AUTO_INIT=1
UC_SPACE_PASSPHRASE=你的空间口令
```

确认后删除错误初始化产生的空数据目录，再重新创建容器。

### 报错 `/data/uniclipboard-server.env: line X: ... not found`

配置文件格式错了。必须是：

```sh
KEY=value
```

不要写：

```text
UC_DEVICE_NAME=Synology Server 这个是设备名称
```

也不要写带空格的未引用命令式内容。

### 手机扫描二维码连接不上

按顺序检查：

1. `UC_MOBILE_PUBLIC_URL` 是否是手机能访问的地址。
2. URL 是否带了正确端口。
3. Nginx 是否转发到容器 `42720`。
4. 访问 `https://你的域名:端口/SyncClipboard.json` 是否返回 `401`。返回 `401` 说明服务通了，只是需要账号密码，这是正常现象。
5. 如果返回超时或 502，说明反代或端口映射不通。

### 桌面端加入失败

桌面端加入需要：

```text
邀请码
Space 空间口令
```

邀请码是临时的，生成后请尽快使用。如果失败，重新在 Web 管理页生成一个新的桌面端邀请。

### 管理页打不开

检查：

```text
UC_ADMIN_WEB=1
UC_ADMIN_PASSWORD=已设置
42888 -> 42888 端口映射存在
```

然后访问：

```text
http://群晖内网IP:42888
```

### 要不要暴露 `42888` 到公网

不建议。管理页能生成设备凭据和桌面端邀请，应该只在内网或 VPN 中访问。
