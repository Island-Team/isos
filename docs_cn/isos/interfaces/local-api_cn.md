# 本地 IsOS v2 API

[English](../../../docs/isos/interfaces/local-api.md)

`isos init` 默认在 `127.0.0.1:16000` 启动 IsOS API（`ISOS_HTTP_PORT` 可调整端口），管理 IsosUI 的 `npm start` 服务并使其监听 `127.0.0.1:16001`（`ISOSUI_PORT` 可调整端口），还会在 `$XDG_RUNTIME_DIR/isos/`（或 `ISOS_HOME/run/`）下创建仅所有者可访问的 Unix 管理套接字。运行目录权限为 `0700`，管理套接字为 `0600`。HTTP 响应封装为 `{schema_version:2,ok,data,error}`；内部单行 Unix 封装仍为 schema 1。

API 只绑定 `127.0.0.1`，不提供前端文件。IsosUI 通过地址和端口直接调用 API，无需 URL 或 bearer 令牌。API `Host` 必须匹配其环回地址和配置的端口。浏览器请求可来自 npm 前端 `127.0.0.1:16001`（或配置的 `ISOSUI_PORT`）、API 自身来源，或托管 IsosUI 来源 `https://www.os-island.com`、`https://os-island.com`；其他 Origin 会被拒绝。跨域预检允许 GET、POST、PUT 和 Content-Type。按 Ctrl+C 会停止两个服务和仅所有者可访问的管理器。

| 路由 | 响应或请求 | 用途 |
| --- | --- | --- |
| `GET /api/isos/v2/node-graph` | `{revision,graph}` | 读取唯一已保存图；无图时返回可编辑空 v2 草稿。 |
| `PUT /api/isos/v2/node-graph` | `{schema_version:2,revision,graph}` | 校验并应用图；修订过期返回 409。 |
| `POST /api/isos/v2/node-graph/validate` | `{schema_version:2,graph}` → `{valid,diagnostics}` | 只校验注册表和软件包，不应用。 |
| `GET /api/isos/v2/node-types` | `{types:[...]}` | 由 `aether-profile` 提供节点类型目录。 |
| `GET /api/isos/v2/node-sets` | `{sets:[{package,id,parameters,nodes}]}` | 已安装软件包提供的节点集合。 |
| `POST /api/isos/v2/node-sets/preview` | `{package:{id,version},set_id,graph,values:{name:string}}` → `{graph,added_paths}` | 将集合合成到草稿，不保存；无效合成返回 422。 |
| `GET /api/isos/v2/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | v2 后端包选择及先决条件检查。 |
| `POST /api/isos/v2/packages/link` | `{id,version,path,manifest?}` | 链接本地包；`manifest` 是可选的 v4 发行 JSON 绝对路径。省略时若存在 `<path>/release.json`，IsOS 会读取它。 |
| `GET /api/isos/v2/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home 配置概况。 |
| `POST /api/isos/v2/process-control` | `{schema_version:2,operation,revision?}` | `run`、`start`、`stop` 或 `restart`；可选修订防止操作过期视图。 |
| `GET /api/isos/v2/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | 固定包与直接子进程状态。 |
| `GET /api/isos/v2/process-logs/{node}?lines=100` | `{node,lines}` | 最近标准输出/错误行，最多 1000 行。 |
| `GET /api/isos/v2/version` | `{api_schema_version,isos_version}` | 工作台兼容性检查。 |

进程管理器在启动前重新校验已保存图和精确版本的软件包。MuJoCo 在 EOS 前启动、在 EOS 后停止。图布局属于编辑器；HTTP 图只包含运行配置和有类型连接。
