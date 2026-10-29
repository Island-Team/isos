# 本地管理 API

IsOS 管理器负责部署图和受控进程状态。命令行与本地 API 使用同一核心；WebUI 使用环回 HTTP 桥接。apply 保存配置，不会重启正在运行的节点。

[English](../../../docs/isos/interfaces/local-api.md)

## 认证与传输

isos daemon 在 $XDG_RUNTIME_DIR/isos/manager.sock 提供仅限当前用户的 Unix socket，一次连接发送一行 JSON 请求并接收一行 JSON 响应。isos ui 在 127.0.0.1:17600 提供 HTTP 桥接，并在终端显示仅本次运行有效的 256 位配对令牌。WebUI 在内存中保存用户粘贴的令牌，请求带 Authorization: Bearer <token>。浏览器 Origin 只允许 https://www.os-island.com、https://os-island.com，或命令行 --allow-origin 指定的精确开发地址。

成功响应为 HTTP 200，格式如下：

    {"schema_version":1,"ok":true,"data":{"started":["eos"]},"error":null}

图校验或生命周期错误为 HTTP 400：

    {"schema_version":1,"ok":false,"data":null,"error":"EOS and EGIA require the same explicit protocol contract"}

缺失令牌返回 401，非法 Origin 或 Host 返回 403。来自允许 Origin 的 OPTIONS 预检返回 204；若请求含 Access-Control-Request-Private-Network: true，响应会包含 Access-Control-Allow-Private-Network: true。请求正文上限为 2 MB。

## GET /api/isos/v1/health

无需令牌，只返回服务名称和版本：

    {"schema_version":1,"ok":true,"data":{"service":"isos","version":"0.1.0"},"error":null}

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

Unix socket 的 op 值为 status、graph、start、stop、apply，请求包含 schema_version:1；响应沿用同一 JSON 包装。isos eos status --json 输出不带包装的状态对象，供 eos-cli 使用。

相关：[部署图](deployment-graph_cn.md)、[发行清单](releases_cn.md)。
