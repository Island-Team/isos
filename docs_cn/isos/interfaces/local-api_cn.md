# IsOS 本地 API

[English](../../../docs/isos/interfaces/local-api.md)

`isos init` 启动一个 IsOS 用户服务：在 `127.0.0.1:16000` 提供 `/api/isos/v2`，固定本地版本后还提供校验过的 IsosUI 页面；并在 `$XDG_RUNTIME_DIR/isos/manager.sock` 提供仅属主可用的控制套接字（父目录 `0700`、套接字 `0600`）。EOS 和 EGIA 不提供配置 HTTP 服务。

服务首次提供本地页面及固定版本改变时会校验页面。每次执行 `isos open` 或 `isos version check` 也会校验页面；固定版本损坏时会报错，不会改为打开托管编辑器。

`isos open` 打开编辑器时不再在 URL 中附带令牌。来自 `http://127.0.0.1:16000`、`https://www.os-island.com` 或 `https://os-island.com` 的浏览器请求可直接访问本机 API。同源浏览器 GET 请求若不含 `Origin`，可由 `Sec-Fetch-Site: same-origin` 识别。其他客户端仍须发送 `Authorization: Bearer <token>`。所有 API 请求都要求准确的 `Host: 127.0.0.1:16000`；未允许的 `Origin` 即使带有令牌也返回 403。CORS 预检允许 GET、POST、PUT、Authorization 和 Content-Type。没有可信浏览器来源或有效令牌的请求返回 401。旧版编辑器仍可提供 URL 片段中的令牌，页面只在内存中保存它。

| 路由 | 用途 |
| --- | --- |
| `GET /api/isos/v2/version` | 读取 `{api_schema_version:2,isos_version}`，供编辑器检查兼容性 |
| `GET /api/isos/v2/graph` | 读取 `{revision,graph}` |
| `POST /api/isos/v2/graph/validate` | 校验 `{schema_version:2,graph}` |
| `PUT /api/isos/v2/graph` | 使用 `{schema_version:2,revision,graph}` 应用；过期版本返回 409 |
| `GET /api/isos/v2/profiles` | 读取 EOS/EGIA 节点描述、有序字段元数据和嵌套默认值 |
| `GET/POST /api/isos/v2/config/{eos|egia}/{operation}` | 调用隔离的组件配置操作 |

成功响应为 `{ "schema_version":2, "ok":true, "data":..., "error":null }`。校验诊断包括 `path`、`code`、`message`。私有 UDS 控制 API 保留 v1 单行 JSON 操作（`status`、`graph`、`start`、`stop`、`apply`），供 CLI 客户端使用。

`data.profiles` 中的每个档案保留 `id`、`version`、`label`、`description`、`inputs` 和 `outputs`。`settings_fields` 是有序数组，字段格式为 `{path,type,label,description?,required?,default?,min?,max?,options?,read_only?}`。点分路径指向嵌套设置；`defaults` 与图中 `settings` 采用相同的嵌套结构。由 IsOS 推导的 EOS UDS 和 EGIA 网关值不放入默认设置。EOS `north_http_url` 由 `host` 和 `north_http_port` 推导。例如，`server.port` 的默认值位于 `defaults.server.port`。适配器可提供更多元数据；此路由提供稳定的编辑器字段子集。

图校验及应用会拒绝适配器无法使用的组件设置，包括未知的嵌套键，并通过组件适配器校验编辑值。EOS 的 `launch.ready_tcp` 必须匹配 `host:grpc_port`，EGIA 必须匹配 `server.host:server.port`，EOS North HTTP 辅助进程的就绪地址必须匹配 `host:north_http_port`。EOS 的 `settings.north_http_url` 必须匹配 HTTP 监听地址。EOS UDS 端点拥有 `settings.socket_path`；IsOS 将该路径传给两个组件适配器。诊断信息尽可能指向相应节点的设置或启动字段。
