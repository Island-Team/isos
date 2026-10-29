# IsOS 本地 API

[English](../../../docs/isos/interfaces/local-api.md)

`isos init` 启动一个 IsOS 用户服务：在 `127.0.0.1:16000` 提供 `/api/isos/v2`，固定本地版本后还提供校验过的 IsosUI 页面；并在 `$XDG_RUNTIME_DIR/isos/manager.sock` 提供仅属主可用的控制套接字（父目录 `0700`、套接字 `0600`）。EOS 和 EGIA 不提供配置 HTTP 服务。

服务首次提供本地页面及固定版本改变时会校验页面。每次执行 `isos open` 或 `isos version check` 也会校验页面；固定版本损坏时会报错，不会改为打开托管编辑器。

`isos open` 将临时令牌放在 URL 片段中。页面立即移除片段，只在内存中保存令牌。HTTP API 要求 `Authorization: Bearer <token>` 和准确的 `Host: 127.0.0.1:16000`。若含 `Origin`，必须为 `http://127.0.0.1:16000`、`https://www.os-island.com` 或 `https://os-island.com`。CORS 预检允许 GET、POST、PUT、Authorization 和 Content-Type。未允许的 Origin 或错误的 Host 返回 403；缺失或无效的令牌返回 401。令牌不会写入浏览器持久存储。

| 路由 | 用途 |
| --- | --- |
| `GET /api/isos/v2/version` | 读取 `{api_schema_version:2,isos_version}`，供编辑器检查兼容性 |
| `GET /api/isos/v2/graph` | 读取 `{revision,graph}` |
| `POST /api/isos/v2/graph/validate` | 校验 `{schema_version:2,graph}` |
| `PUT /api/isos/v2/graph` | 使用 `{schema_version:2,revision,graph}` 应用；过期版本返回 409 |
| `GET /api/isos/v2/profiles` | 读取 EOS/EGIA 节点描述 |
| `GET/POST /api/isos/v2/config/{eos|egia}/{operation}` | 调用隔离的组件配置操作 |

成功响应为 `{ "schema_version":2, "ok":true, "data":..., "error":null }`。校验诊断包括 `path`、`code`、`message`。私有 UDS 控制 API 保留 v1 单行 JSON 操作（`status`、`graph`、`start`、`stop`、`apply`），供 CLI 客户端使用。
