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
| `GET /api/isos/v2/build-binaries` | `{selected:receipt|null,source_current:boolean,stale_reason:string|null}` | 已选且摘要校验通过的 EOS 程序及源码状态。源码漂移作为状态返回，不导致 HTTP 错误；未选择时 `source_current:false` 且 `stale_reason:null`。 |
| `POST /api/isos/v2/build-binaries` | `{schema_version:2,target:"eos",install_deps:false}` → `{receipt,suggested_graph}` | 构建并暂存 Rust EOS 程序，不改变已选构建；预览建议图后调用选择接口。 |
| `POST /api/isos/v2/build-binaries/select` | `{schema_version:2,selector,revision}` → `{receipt,graph:{revision,graph}}` | 选择已校验的 EOS 暂存构建；仅当图修订仍匹配时应用三个进程命令。 |
| `GET /api/isos/v2/node-sets` | `{sets:[{package,id,parameters,nodes}]}` | 已安装软件包提供的节点集合。 |
| `POST /api/isos/v2/node-sets/preview` | `{package:{id,version},set_id,graph,values:{name:string}}` → `{graph,added_paths}` | 将集合合成到草稿，不保存；无效合成返回 422。 |
| `GET /api/isos/v2/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | v2 后端包选择及先决条件检查。 |
| `POST /api/isos/v2/packages/link` | `{id,version,path,manifest?}` | 链接本地包；`manifest` 是可选的 v4 发行 JSON 绝对路径。省略时若存在 `<path>/release.json`，IsOS 会读取它。 |
| `GET /api/isos/v2/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home 配置概况。 |
| `POST /api/isos/v2/process-control` | `{schema_version:2,operation,revision?}` | `run`、`start`、`stop` 或 `restart`；可选修订防止操作过期视图。 |
| `GET /api/isos/v2/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | 固定包与直接子进程状态。 |
| `GET /api/isos/v2/process-logs/{node}?lines=100` | `{node,lines}` | 结构化消息的兼容视图，最多 1000 行。 |
| `GET /api/isos/v2/logs?source=all&cursor=...&limit=100&level=info` | `{lines:[{timestamp,level,source,event_code,message,pid}],next_cursor,rotated}` | 读取统一轮转 JSON 日志，并按 `level` 过滤。每次最多 200 行或 64 KiB；文件轮转时 `rotated` 为 true。 |
| `GET /api/isos/v2/version` | `{api_schema_version,isos_version}` | 工作台兼容性检查。 |

进程管理器在启动前重新校验已保存图和精确版本的软件包。EOS 将 MuJoCo 作为直接子进程启动，并在停止获准的执行器后终止它。图布局属于编辑器；HTTP 图只包含运行配置和有类型连接。

## 仅所有者可访问的管理器操作

Unix 套接字每行接收一个 JSON 请求：`{schema_version:1,op,deployment?,revision?,service_id?,cursor?,limit?}`，返回 `{schema_version:1,ok,data,error}`。管理器处理前检查对端 UID。`manager_identity` 返回 `pid`（进程 ID）、`start_ticks`（Linux 进程启动身份）、`mode`（`dev` 或 `product`）、`executable`（绝对路径）、`executable_sha256`（摘要）、IsOS/API/服务契约版本、`api_origin` 和 `ui_url`。用该身份区分已有的 IsOS 管理器与外部端口监听者；单凭端口号不能判定进程所有权。

`manager_shutdown` 停止所有图子进程和独立服务，然后结束 API、界面及管理器。`v2_stop` 只影响指定部署。`services_list`、`services_validate`、`services_status`、`services_start`、`services_stop`、`services_restart` 和 `services_logs` 操作[独立服务定义](services_cn.md)；启动、停止、重启和日志请求需要 `service_id`，日志可带不透明的 `cursor` 及 1–200 的 `limit`。可执行文件摘要不同的 CLI 仍可请求关停，但不能启动或重启部署和服务。`isos status --json` 汇总部署状态、管理器身份和服务状态；`isos version check` 比较配置、选中与运行中的身份。

日志 `source` 可为 `all`、`isos`、`services`、`service-gateway` 等服务来源、`packages`、`graph`、`ui`、`eos`、`egia`、`mujoco`、`bridge`、`config` 或通过校验的子进程 ID。`limit` 范围为 1–200，默认为 100。每行包含 RFC 3339 UTC `timestamp`、`level`、`source`、`message`，并可包含 `event_code` 和 `pid`。IsOS 记录自身诊断及选定的结构化子进程事件，不镜像普通标准输出或标准错误。即使当前文件已读完，`next_cursor` 仍为字符串，后续可用它继续轮询。日志目录权限为 `0700`、普通文件为 `0600`；达到 `[logging].max_bytes`（默认 1 MiB）后轮转，保留 `[logging].backups`（默认 3）份。旧进程端点从统一日志读取消息。

`[logging].level` 按 `trace < debug < info < warn < error` 记录达到阈值的文件事件，默认为 `info`。HTTP 的 `level` 参数和 UI 选择器只过滤显示，不改变写入。终端状态和命令错误不受影响。`isos logs [source] --level warn` 读取同一日志库。

外部核心可通过仅所有者访问的管理套接字提交选定事件：`{schema_version:1,op:"log_append",deployment?,log:{source,level,message,event_code?}}`，响应含 `{accepted:true}`。`source` 为经校验的子进程 ID（1–48 个 ASCII 字母、数字、`_` 或 `-`），不能是 `all`、`events`、`services`、`isos` 或 `browser`；`level` 为五种级别之一；`message` 为 1–8192 个 UTF-8 字节；可选 `event_code` 为 1–64 个小写 ASCII 字母、数字或下划线。核心仍负责自己的完整日志。

应用图包含 MuJoCo 软件包时，IsOS 会把部署日志目录传给 EOS 运行时。只有 EOS 写入和轮转 `mujoco.log`，并遵守相同的 JSON 行与权限契约；IsOS 通过此 API 读取。

### 本地运行配置

IsOS 默认读取其源码目录中的 [config/isos.toml](../../../config/isos.toml)，与命令工作目录无关。`ISOS_CONFIG` 可指定必须存在的其他绝对配置文件路径。环境变量覆盖相应 TOML 字段；TOML 中的相对源码路径以 IsOS 源码目录为基准。私有部署状态仍使用 `ISOS_HOME` 或 XDG 目录。不会从命令工作目录搜索 `.env`。参见 [config/isos.example.toml](../../../config/isos.example.toml)。

| TOML 字段 | 类型／单位 | 用途与校验 | 环境变量覆盖 |
| --- | --- | --- | --- |
| `server.http_port` | 整数 TCP 端口 | IsOS 环回 API；1024–65535 | `ISOS_HTTP_PORT` |
| `server.ui_port` | 整数 TCP 端口 | IsosUI 环回前端；1024–65535，且不同于 HTTP 端口 | `ISOSUI_PORT` |
| `paths.ui_repo` | 路径字符串 | 可选的 IsosUI 源码目录，用于构建或显式 `--ui-live`；默认使用 IsOS 旁的 `IsosUI`；相对路径以 IsOS 源码目录为基准 | `ISOSUI_REPO_PATH` |
| `paths.aether_bin_dir` | 路径字符串 | Aether Rust 可执行文件目录；相对路径以 IsOS 源码目录为基准 | `ISOS_AETHER_BIN_DIR` |
| `paths.mujoco_root` | 路径字符串 | 可选 MuJoCo 运行根目录；相对路径以 IsOS 源码目录为基准 | `MUJOCO_ROOT` |
| `deployment.name` | slug 字符串 | 默认图与日志命名空间；需通过部署名校验 | `ISOS_DEPLOYMENT` |
| `logging.max_bytes` | 整数，字节 | 轮转阈值 65,536–100,000,000；默认 1,048,576 | `ISOS_LOG_MAX_BYTES` |
| `logging.backups` | 整数，份数 | 轮转副本 1–10；默认 3 | `ISOS_LOG_BACKUPS` |
| `logging.level` | 枚举字符串 | 文件最低级别：`trace`、`debug`、`info`、`warn` 或 `error`；默认 `info` | `ISOS_LOG_LEVEL` |

## Rust 程序与选择器契约

`receipt` 包含 `schema_version`（整数 2）、`component`（`eos`）、`product_version`（`2.0.0`）、`source_commit`（完整 Git SHA）、`source_dirty`（源码有修改或未跟踪文件时为 true）、`source_fingerprint`（提交和源码文件的 SHA-256）、`source_root`（用于开发环境漂移检查的可选绝对源码路径）、`binaries`（按程序名索引的绝对路径）和 `sha256`（文件摘要）。构建会将五个 Aether 程序作为同一集合放入 IsOS 私有数据目录，但不会改变已选构建。产品模式仅接纳干净且带源码指纹的收据。Run 会探测 EOS、bridge、config 与 profile 的精确能力；程序过旧或图指向其他程序时直接报错。`suggested_graph` 为 `{revision,graph}` 或 null，不会自动应用。CLI `isos build eos` 使用同一构建服务；选择接口或 `isos version select eos <selector>` 校验图修订后再应用三个程序路径和固定选择。选择器接受唯一的提交前缀、产品版本或源码指纹前缀；同一提交有多个构建时使用指纹。

在开发模式下，EOS 源码变化后，图编辑与校验继续使用摘要校验通过的已选 `aether-profile`。选择暂存构建时，先使用候选构建的 `aether-profile` 校验建议图，再更新图和固定选择；Run 仍要求源码与所选构建一致。版本检查使用 `match.eos_source`（布尔值；未选择时为 null）和 `eos_source_diagnostic`（字符串或 null）报告源码漂移。

IsOS 和 IsosUI 构建使用独立的不可变收据，并记录相同的源码身份字段。`isos build isos` 暂存 release 可执行文件；`isos version select isos <commit>` 要求先停止管理器，再激活该程序。IsosUI 收据还将目录摘要绑定到清单；产品模式仅从已选择且校验通过的目录提供页面。

`adapter_families` 包含 `id`（稳定适配器 ID）、`device_kinds`（设备类别）、可选精确版本 `package:{id,version}`、布尔值 `compiled`／`available`、字符串 `transport`／`description`、可选 `supported_primitives` 及不可用时的 `reason`。`primitive_bindings` 包含 `node_type`、`device_kind`、`movement_types`、`available`、`reason`、以及具有精确名称、版本、运动类型和描述的 `primitives`。`targets` 包含四段式 `path`、`node_type`、`device_kind`、`adapter_type`、`movement_types`、`available`、`reason`。`GET /node-types` 同时透传 EOS 的 `primitive_specs` 与有类型的 `data_types`。
