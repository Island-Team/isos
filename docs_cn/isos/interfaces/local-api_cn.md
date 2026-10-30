# 本地 IsOS v2 API

[English](../../../docs/isos/interfaces/local-api.md)

`isos init --mode dev|product` 创建配置和初始图后退出。`isos serve` 默认在 `127.0.0.1:16000` 启动 IsOS API（`ISOS_HTTP_PORT` 可调整端口），在 `127.0.0.1:16001` 提供选定的静态 IsosUI 构建（`ISOSUI_PORT` 可调整端口），并在 `$XDG_RUNTIME_DIR/isos/`（或 `ISOS_HOME/run/`）下创建仅所有者可访问的 Unix 管理套接字。开发模式下的 `isos serve --ui-live` 才会管理 Webpack。运行目录权限为 `0700`，管理套接字为 `0600`。HTTP 响应封装为 `{schema_version:2,ok,data,error}`；内部单行 Unix 封装仍为 schema 1。

API 只绑定 `127.0.0.1`，不提供前端文件。IsosUI 通过地址和端口直接调用 API，无需 URL 或 bearer 令牌。API `Host` 必须匹配其环回地址和配置的端口。浏览器请求可来自 UI 来源 `127.0.0.1:16001`（或配置的 `ISOSUI_PORT`）、API 自身来源，或托管 IsosUI 来源 `https://www.os-island.com`、`https://os-island.com`；其他 Origin 会被拒绝。跨域预检允许 GET、POST、PUT 和 Content-Type。Ctrl+C 或 `isos stop` 会停止两个服务、部署、显式启动的服务以及管理器。

| 路由 | 响应或请求 | 用途 |
| --- | --- | --- |
| `GET /api/isos/v2/node-graph` | `{revision,graph}` | 读取唯一已保存图；无图时返回可编辑空 v2 草稿。 |
| `PUT /api/isos/v2/node-graph` | `{schema_version:2,revision,graph}` | 校验并应用图；修订过期返回 409。 |
| `POST /api/isos/v2/node-graph/validate` | `{schema_version:2,graph}` → `{valid,diagnostics}` | 只校验注册表和软件包，不应用。 |
| `GET /api/isos/v2/node-types` | `{types:[...]}` | 由 `aether-profile` 提供节点类型目录。 |
| `GET /api/isos/v2/node-options` | `{adapter_families,primitive_bindings,targets}` | 已保存图的适配器、原语绑定与目标选项。 |
| `POST /api/isos/v2/node-options` | `{schema_version:2,graph}` → `{adapter_families,primitive_bindings,targets}` | 未应用草稿的选择器选项。 |
| `GET /api/isos/v2/build-binaries` | `{selected:receipt|null}` | 已暂存的精确 Rust EOS 二进制集合。 |
| `POST /api/isos/v2/build-binaries` | `{schema_version:2,target:"eos",install_deps:false}` → `{receipt,suggested_graph}` | 构建并暂存 Rust EOS 程序，不改变已选构建；预览建议图后调用选择接口。 |
| `POST /api/isos/v2/build-binaries/select` | `{schema_version:2,selector,revision}` → `{receipt,graph:{revision,graph}}` | 选择已校验的 EOS 暂存构建；仅当图修订仍匹配时应用三个进程命令。 |
| `GET /api/isos/v2/node-sets` | `{sets:[{package,id,parameters,nodes}]}` | 已安装软件包提供的节点集合。 |
| `POST /api/isos/v2/node-sets/preview` | `{package:{id,version},set_id,graph,values:{name:string}}` → `{graph,added_paths}` | 将集合合成到草稿，不保存；无效合成返回 422。 |
| `GET /api/isos/v2/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | v2 后端包选择及先决条件检查。 |
| `POST /api/isos/v2/packages/link` | `{id,version,path,manifest?}` | 链接本地包；`manifest` 是可选的 v4 发行 JSON 绝对路径。省略时若存在 `<path>/release.json`，IsOS 会读取它。 |
| `GET /api/isos/v2/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home 配置概况。 |
| `POST /api/isos/v2/process-control` | `{schema_version:2,operation,revision?}` | `run`、`start`、`stop` 或 `restart`；可选修订防止操作过期视图。 |
| `GET /api/isos/v2/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | 固定包与直接子进程状态。 |
| `GET /api/isos/v2/process-logs/{node}?lines=100` | `{node,lines}` | 最近标准输出/错误行，最多 1000 行。 |
| `GET /api/isos/v2/logs?source=all&cursor=...&limit=100` | `{lines:[{timestamp,level,source,event_code,message,pid}],next_cursor,rotated}` | 读取统一轮转 JSON 日志；仍可选择 `source=isos` 或进程来源。每次最多 200 行或 64 KiB；文件轮转时 `rotated` 为 true。UI 的级别选择只过滤显示。 |
| `GET /api/isos/v2/version` | `{api_schema_version,isos_version}` | 工作台兼容性检查。 |

进程管理器在启动前重新校验已保存图和精确版本的软件包。EOS 将 MuJoCo 作为直接子进程启动，并在停止获准的执行器后终止它。图布局属于编辑器；HTTP 图只包含运行配置和有类型连接。

日志 `source` 可为 `isos`、`packages`、`graph`、`ui`、`eos`、`egia`、`mujoco`、`bridge`、`config` 或通过校验的子进程 ID。`limit` 范围为 1–200，默认为 100。每行包含 RFC 3339 UTC 字符串 `timestamp`、字符串 `level`（子进程标准输出为 `info`，错误输出为 `error`）、字符串 `source` 和字符串 `message`。即使当前文件已读完，`next_cursor` 仍为字符串，后续可用它继续轮询。日志目录权限为 `0700`、普通文件为 `0600`；达到 `[logging].max_bytes`（默认 1 MiB）后轮转，保留 `[logging].backups`（默认 3）份。旧进程端点读取镜像原始文本行。

`[logging].level` 按 `trace < debug < info < warn < error` 过滤低于阈值的文件记录，默认为 `info`。子进程标准输出为 `info`，标准错误为 `error`；选 `warn` 或 `error` 时，分类文件及旧日志文件均省略标准输出但保留错误。终端状态和命令错误不受影响。

应用图包含 MuJoCo 软件包时，IsOS 会把部署日志目录传给 EOS 运行时。只有 EOS 写入和轮转 `mujoco.log`，并遵守相同的 JSON 行与权限契约；IsOS 通过此 API 读取。

### 本地运行配置

IsOS 会读取存在的 `~/.config/isos/config.toml`（或 `ISOS_HOME/config/config.toml`）。`ISOS_CONFIG` 可指定必须存在的其他绝对路径。环境变量覆盖相应 TOML 字段。不会从命令工作目录搜索 `.env`。参见 [config.example.toml](../../../config.example.toml)。

| TOML 字段 | 类型／单位 | 用途与校验 | 环境变量覆盖 |
| --- | --- | --- | --- |
| `server.http_port` | 整数 TCP 端口 | IsOS 环回 API；1024–65535 | `ISOS_HTTP_PORT` |
| `server.ui_port` | 整数 TCP 端口 | IsosUI 环回前端；1024–65535，且不同于 HTTP 端口 | `ISOSUI_PORT` |
| `paths.ui_repo` | 绝对路径字符串 | 已安装 npm 依赖的 IsosUI 仓库 | `ISOSUI_REPO_PATH` |
| `paths.aether_bin_dir` | 绝对路径字符串 | Aether Rust 可执行文件目录 | `ISOS_AETHER_BIN_DIR` |
| `paths.mujoco_root` | 绝对路径字符串 | 可选 MuJoCo 运行根目录 | `MUJOCO_ROOT` |
| `deployment.name` | slug 字符串 | 默认图与日志命名空间；需通过部署名校验 | `ISOS_DEPLOYMENT` |
| `logging.max_bytes` | 整数，字节 | 轮转阈值 65,536–100,000,000；默认 1,048,576 | `ISOS_LOG_MAX_BYTES` |
| `logging.backups` | 整数，份数 | 轮转副本 1–10；默认 3 | `ISOS_LOG_BACKUPS` |
| `logging.level` | 枚举字符串 | 文件最低级别：`trace`、`debug`、`info`、`warn` 或 `error`；默认 `info` | `ISOS_LOG_LEVEL` |

## Rust 程序与选择器契约

`receipt` 包含 `schema_version`（整数 2）、`component`（`eos`）、`product_version`（`2.0.0`）、`source_commit`（完整 Git SHA）、`source_dirty`（源码有修改或未跟踪文件时为 true）、`source_fingerprint`（提交和源码文件的 SHA-256）、`source_root`（用于开发环境漂移检查的可选绝对源码路径）、`binaries`（按程序名索引的绝对路径）和 `sha256`（文件摘要）。构建会将五个 Aether 程序作为同一集合放入 IsOS 私有数据目录，但不会改变已选构建。产品模式仅接纳干净且带源码指纹的收据。Run 会探测 EOS、bridge、config 与 profile 的精确能力；程序过旧或图指向其他程序时直接报错。`suggested_graph` 为 `{revision,graph}` 或 null，不会自动应用。CLI `isos build eos` 使用同一构建服务；选择接口或 `isos version select eos <commit>` 校验图修订后再应用三个程序路径和固定选择。

IsOS 和 IsosUI 构建使用独立的不可变收据，并记录相同的源码身份字段。`isos build isos` 暂存 release 可执行文件；`isos version select isos <commit>` 要求先停止管理器，再激活该程序。IsosUI 收据还将目录摘要绑定到清单；产品模式仅从已选择且校验通过的目录提供页面。

`adapter_families` 包含 `id`（稳定适配器 ID）、`device_kinds`（设备类别）、可选精确版本 `package:{id,version}`、布尔值 `compiled`／`available`、字符串 `transport`／`description`、可选 `supported_primitives` 及不可用时的 `reason`。`primitive_bindings` 包含 `node_type`、`device_kind`、`movement_types`、`available`、`reason`、以及具有精确名称、版本、运动类型和描述的 `primitives`。`targets` 包含四段式 `path`、`node_type`、`device_kind`、`adapter_type`、`movement_types`、`available`、`reason`。`GET /node-types` 同时透传 EOS 的 `primitive_specs` 与有类型的 `data_types`。
