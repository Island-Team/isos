# 本地管理 API

IsOS 命令行直接调用管理器核心。其他本地客户端可使用仅限当前用户访问的 Unix socket。旧版环回 HTTP/WebSocket 桥接服务已移除。

[English](../../../docs/isos/interfaces/local-api.md)

## 认证与传输

运行 `isos daemon` 后，服务会监听 `$XDG_RUNTIME_DIR/isos/manager.sock`。父目录权限为 `0700`，socket 权限为 `0600`。每个连接发送一行 JSON 请求并接收一行 JSON 响应。

请求使用 `schema_version: 1`，`op` 可为 `status`、`graph`、`start`、`stop` 或 `apply`。部署名称默认读取 `ISOS_DEPLOYMENT`，若未设置则为 `default`；组件操作可将 `role` 设为 `eos` 或 `egia`。

    {"schema_version":1,"op":"status","deployment":"default","role":"eos"}

成功操作使用版本化响应包装：

    {"schema_version":1,"ok":true,"data":{"started":["eos"]},"error":null}

校验失败时仍使用相同包装，设置 `ok: false`、`data: null`，并在 `error` 中说明原因。CLI 的 `isos eos status --json` 为兼容 `eos-cli` 返回不带包装的状态数据。

Socket 支持查询部署状态、读取/应用配置图以及启动/停止操作。应用配置会校验并格式化完整配置图、生成激活模板与 TOML，但不会重启运行中的节点。启动和停止遵循依赖顺序及记录的活动配置快照。

相关：[部署图](deployment-graph_cn.md)、[发行清单](releases_cn.md)。
