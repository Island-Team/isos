# IsOS 本地 API

[English](../../../docs/isos/interfaces/local-api.md)

`isos init` 启动一个 IsOS 用户服务：在 `127.0.0.1:16000` 提供独立 Island-web 编辑器和 `/api/isos/v2`，并在 `$XDG_RUNTIME_DIR/isos/manager.sock` 提供仅属主可用的控制套接字（父目录 `0700`、套接字 `0600`）。EOS 和 EGIA 不提供配置 HTTP 服务。

`isos open` 将临时令牌放在 URL 片段中。页面立即移除片段，只在内存中保存令牌。HTTP API 要求 `Authorization: Bearer <token>`、准确的 `Host: 127.0.0.1:16000`，若含 `Origin` 则必须为 `http://127.0.0.1:16000`。令牌不会写入浏览器持久存储。

| 路由 | 用途 |
| --- | --- |
| `GET /api/isos/v2/graph` | 读取 `{revision,graph}` |
| `POST /api/isos/v2/graph/validate` | 校验 `{schema_version:2,graph}` |
| `PUT /api/isos/v2/graph` | 使用 `{schema_version:2,revision,graph}` 应用；过期版本返回 409 |
| `GET /api/isos/v2/profiles` | 读取 EOS/EGIA 节点描述 |
| `GET/POST /api/isos/v2/config/{eos|egia}/{operation}` | 调用隔离的组件配置操作 |

成功响应为 `{ "schema_version":2, "ok":true, "data":..., "error":null }`。校验诊断包括 `path`、`code`、`message`。私有 UDS 控制 API 保留 v1 单行 JSON 操作（`status`、`graph`、`start`、`stop`、`apply`），供 CLI 客户端使用。
