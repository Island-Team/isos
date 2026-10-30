# 本地 IsOS v2 API

[English](../../../docs/isos/interfaces/local-api.md)

`isos init` 默认在 `127.0.0.1:16000` 启动本地工作台服务（`ISOS_HTTP_PORT` 可为隔离测试选择另一个环回端口），并在 `$XDG_RUNTIME_DIR/isos/`（或 `ISOS_HOME/run/`）下创建仅所有者可访问的 Unix 管理套接字。运行目录权限为 `0700`，管理套接字为 `0600`。HTTP 响应封装为 `{schema_version:2,ok,data,error}`；内部单行 Unix 封装仍为 schema 1。

服务接受同源请求及托管 IsosUI 来源 `https://www.os-island.com`、`https://os-island.com`；其他客户端须提供 bearer 令牌。`Host` 必须为 `127.0.0.1:16000`。跨域预检允许 GET、POST、PUT、Authorization 和 Content-Type。

| 路由 | 响应或请求 | 用途 |
| --- | --- | --- |
| `GET /api/isos/v2/node-graph` | `{revision,graph}` | 读取唯一已保存图；无图时返回可编辑空 v2 草稿。 |
| `PUT /api/isos/v2/node-graph` | `{schema_version:2,revision,graph}` | 校验并应用图；修订过期返回 409。 |
| `POST /api/isos/v2/node-graph/validate` | `{schema_version:2,graph}` → `{valid,diagnostics}` | 只校验注册表和软件包，不应用。 |
| `GET /api/isos/v2/node-types` | `{types:[...]}` | 由 `aether-profile` 提供节点类型目录。 |
| `GET /api/isos/v2/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | v2 后端包选择及先决条件检查。 |
| `GET /api/isos/v2/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home 配置概况。 |
| `POST /api/isos/v2/process-control` | `{schema_version:2,operation,revision?}` | `run`、`start`、`stop` 或 `restart`；可选修订防止操作过期视图。 |
| `GET /api/isos/v2/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | 固定包与直接子进程状态。 |
| `GET /api/isos/v2/process-logs/{node}?lines=100` | `{node,lines}` | 最近标准输出/错误行，最多 1000 行。 |
| `GET /api/isos/v2/version` | `{api_schema_version,isos_version}` | 工作台兼容性检查。 |

进程管理器在启动前重新校验已保存图和精确版本的软件包。MuJoCo 在 EOS 前启动、在 EOS 后停止。图布局属于编辑器；HTTP 图只包含运行配置和有类型连接。
