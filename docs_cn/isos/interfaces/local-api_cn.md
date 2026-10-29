# 本地管理 API

IsOS 管理器负责部署图和受控进程状态。命令行与本地 API 使用同一核心；WebUI 使用环回 HTTP 桥接。apply 保存配置，不会重启正在运行的节点。

[English](../../../docs/isos/interfaces/local-api.md)

## 认证与传输

isos daemon 在 $XDG_RUNTIME_DIR/isos/manager.sock 提供仅限当前用户的 Unix socket，一次连接发送一行 JSON 请求并接收一行 JSON 响应。isos ui 在 127.0.0.1:17600 提供 HTTP 桥接，并在终端显示仅本次运行有效的 256 位配对令牌，以及把令牌放在 URL 片段中的本地页面链接。页面立即移除片段，仅在当前标签页内存保存令牌。托管 WebUI 仍让用户手工粘贴令牌。HTTP 请求带 Authorization: Bearer <token>；EGIA WebSocket 将令牌作为首个消息认证，因为浏览器 WebSocket API 不支持自定义授权请求头。浏览器 Origin 只允许精确本地地址、https://www.os-island.com、https://os-island.com，或命令行 --allow-origin 指定的精确开发地址。

成功响应为 HTTP 200，格式如下：

    {"schema_version":1,"ok":true,"data":{"started":["eos"]},"error":null}

图校验或生命周期错误为 HTTP 400：

    {"schema_version":1,"ok":false,"data":null,"error":"EOS and EGIA require the same explicit protocol contract"}

缺失令牌返回 401，非法 Origin 或 Host 返回 403。来自允许 Origin 的 OPTIONS 预检返回 204；若请求含 Access-Control-Request-Private-Network: true，响应会包含 Access-Control-Allow-Private-Network: true。请求正文上限为 2 MB。

## GET /api/isos/v1/health

无需令牌，只返回服务名称和版本：

    {"schema_version":1,"ok":true,"data":{"service":"isos","version":"0.1.0"},"error":null}

## 本地页面与软件包读取

GET /、/ui.js 和 /ui.css 提供不含令牌的静态资源，无需 Bearer 令牌，但仍检查精确 Host 和允许的 Origin。资源设置 no-store、no-referrer 和严格的内容安全策略；页面不加载远程资源。URL 片段不会进入 HTTP 请求。

GET /api/isos/v1/packages 返回已安装软件包记录数组。GET /api/isos/v1/catalog?q=mujoco 搜索审核目录并返回其数据。两者都需要 Bearer 令牌，使用相同的响应包装；查询格式与 isos packages search 一致。目录错误返回 HTTP 400。

## GET /api/isos/v1/status

配对后的 WebUI 通过 ?deployment=default&role=eos 查询状态；role 可省略以查询完整部署。

    {"schema_version":1,"ok":true,"data":{"schema_version":1,"active":"default","north_http_url":"http://127.0.0.1:16005","running":true,"nodes":[{"id":"eos","running":true}]},"error":null}

EOS 离线时 active 为 null、north_http_url 为空、running 为 false。

## GET /api/isos/v1/graph

通过 ?deployment=default 读取已应用的完整部署图；尚无部署图时返回 400。

    {"schema_version":1,"ok":true,"data":{"schema_version":1,"name":"default","nodes":[],"edges":[]},"error":null}

## POST /api/isos/v1/apply

发送完整图；成功后保存配置并生成 TOML，不重启现有节点。错误图返回 400。

    {"schema_version":1,"deployment":"default","graph":{"schema_version":1,"name":"default","nodes":[],"edges":[]}}
    {"schema_version":1,"ok":true,"data":{"applied":"default"},"error":null}

## POST /api/isos/v1/start

启动完整部署或指定组件的依赖闭包。缺失软件包、systemd 不可用或就绪失败返回 400。

    {"schema_version":1,"deployment":"default","role":"eos"}
    {"schema_version":1,"ok":true,"data":{"started":["eos"]},"error":null}

## POST /api/isos/v1/stop

逆依赖顺序关闭 IsOS 拥有的节点；共享服务仍被其他运行节点使用时会保留。

    {"schema_version":1,"deployment":"default","role":"eos"}
    {"schema_version":1,"ok":true,"data":{"stopped":["egia","eos"]},"error":null}

## GET /api/isos/v1/egia/ws

配对后的浏览器通过此本地 WebSocket 桥接 EGIA 操作控制台。浏览器先完成 WebSocket 升级；该请求仍经过现有的精确 Origin 与 Host 检查，然后在 10 秒内发送一个 JSON 文本帧：

    {"type":"authenticate","token":"<配对令牌>","deployment":"default","route":"/api/aether/egia/health"}

桥接服务会先验证配对令牌，再打开上游连接。`deployment` 默认是 `default`；所选且已验证的配置图必须包含一个已启用、具有 `launch.ready_tcp` 的 EGIA 组件。IsOS 只接受解析结果全为回环地址的目标。浏览器不能指定上游主机、端口、URL，也不能使用固定 EGIA 路由白名单以外的路由。白名单包括健康状态、后端、聊天、任务流/命令、审核、上下文、模型 I/O、提示词、原语、技能/命令/执行、感知、世界状态、检测、ASR 与原语调用。

连接到配置的 EGIA 监听端点后，IsOS 发送 `{"type":"ready","route":"/api/aether/egia/health"}`，再双向转发文本、二进制、ping、pong 和 close 帧。就绪前发生错误时，服务发送例如 `{"type":"error","error":"pairing token rejected"}` 的 JSON 帧并关闭连接。令牌仅在首帧发送并只保存在浏览器内存中；IsOS 连接 EGIA 上游时不发送 Origin，因此不会更改 EGIA 的 Origin allowlist。

桥接服务将首帧等待时间限制为 10 秒，并将 WebSocket 消息/帧限制为 1 MiB。客户端断开时会关闭上游连接。IsOS 监听器仍仅绑定 `127.0.0.1`；托管 HTTPS 页面连接本地 WebSocket 时，浏览器仍可能执行本地网络或回环访问策略。

Unix socket 的 op 值为 status、graph、start、stop、apply，请求包含 schema_version:1；响应沿用同一 JSON 包装。isos eos status --json 输出不带包装的状态对象，供 eos-cli 使用。

相关：[部署图](deployment-graph_cn.md)、[发行清单](releases_cn.md)。
