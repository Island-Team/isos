# 本地 IsOS v1 API

[English](../../../docs/isos/interfaces/local-api.md)

`isos init --mode dev|product` 在缺少配置或初始图时创建它们。若图曾应用，则校验已保存的 v1 图，并根据已配置的程序目录刷新生成的核心配置和程序摘要；不会修改图或启动进程。旧版图或不兼容核心会中止刷新并给出指引，图文件保持原样。`isos serve` 默认在 `127.0.0.1:16000` 启动 IsOS API（`ISOS_HTTP_PORT` 可调整端口），在 `127.0.0.1:16001` 提供已配置的预构建 IsosUI 文件（`ISOSUI_PORT` 可调整端口），并在 `$XDG_RUNTIME_DIR/isos/`（或 `ISOS_HOME/run/`）下创建仅所有者可访问的 Unix 管理套接字。`isos serve --ui-live` 才会管理 Webpack。运行目录权限为 `0700`，管理套接字为 `0600`。HTTP 响应封装为 `{schema_version:1,ok,data,error}`；内部单行 Unix 封装仍为 schema 1。

API 只绑定 `127.0.0.1`，不提供前端文件。IsosUI 通过地址和端口直接调用 API，无需 URL 或 bearer 令牌。API `Host` 必须匹配其环回地址和配置的端口。浏览器请求可来自 UI 来源 `127.0.0.1:16001`（或配置的 `ISOSUI_PORT`）、API 自身来源，或托管 IsosUI 来源 `https://www.os-island.com`、`https://os-island.com`；其他 Origin 会被拒绝。跨域预检允许 GET、POST、PUT 和 Content-Type。Ctrl+C 或 `isos stop` 会停止两个服务、部署、显式启动的服务以及管理器。

| 路由 | 响应或请求 | 用途 |
| --- | --- | --- |
| `GET /api/isos/v1/node-graph` | `{revision,graph}` | 读取唯一已保存图；无图时返回可编辑空 v1 草稿。 |
| `PUT /api/isos/v1/node-graph` | `{schema_version:1,revision,graph}` | 校验并应用图；修订过期返回 409。 |
| `POST /api/isos/v1/node-graph/validate` | `{schema_version:1,graph}` → `{valid,diagnostics}` | 只校验注册表和软件包，不应用。 |
| `GET /api/isos/v1/node-types` | `{types:[...]}` | 由 `aether-profile` 提供节点类型目录。 |
| `GET /api/isos/v1/core-setup/{role}/catalog` | 含 `catalog` 的配置响应 | 查询已配置的 `eos` 或 `egia` 适配器，不启动核心运行时。 |
| `POST /api/isos/v1/core-setup/{role}/validate` | `{schema_version:1,graph,revision?}` → 配置响应 | 按核心规则校验编辑草稿，不保存。 |
| `POST /api/isos/v1/core-setup/{role}/render` | `{schema_version:1,graph,revision?}` → 配置响应 | 预览生成的核心配置；API 不写入预览。 |
| `GET /api/isos/v1/node-options` | `{adapter_families,primitive_bindings,targets}` | 已保存图的适配器、原语绑定与目标选项。 |
| `POST /api/isos/v1/node-options` | `{schema_version:1,graph}` → `{adapter_families,primitive_bindings,targets}` | 未应用草稿的选择器选项。 |
| `GET /api/isos/v1/core-readiness` | `{cores,graph,isos,isosui}` | 检查已配置核心路径、兼容性、核心是否在已保存节点图中启用、图配置、已配置端点与运行状态。端点表示配置，不代表正在监听。 |
| `GET /api/isos/v1/node-sets` | `{sets:[{package,id,parameters,nodes}]}` | 已安装软件包提供的节点集合。 |
| `POST /api/isos/v1/node-sets/preview` | `{package:{id,version},set_id,graph,values:{name:string}}` → `{graph,added_paths}` | 将集合合成到草稿，不保存；无效合成返回 422。 |
| `GET /api/isos/v1/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | v2 后端包选择及先决条件检查。 |
| `POST /api/isos/v1/packages/link` | `{id,version,path,manifest?}` | 链接本地包；`manifest` 是可选的 v4 发行 JSON 绝对路径。省略时若存在 `<path>/release.json`，IsOS 会读取它。 |
| `GET /api/isos/v1/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home 配置概况。 |
| `POST /api/isos/v1/process-control` | `{schema_version:1,operation,revision?}` | `run`、`start`、`stop` 或 `restart`；可选修订防止操作过期视图。 |
| `GET /api/isos/v1/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | 固定包与直接子进程状态。 |
| `GET /api/isos/v1/logs?source=all&cursor=...&limit=100&level=info` | `{lines:[{timestamp,level,source,event_code,message,pid}],next_cursor,rotated}` | 读取统一轮转 JSON 日志，并按 `level` 过滤。每次最多 200 行或 64 KiB；文件轮转时 `rotated` 为 true。 |
| `GET /api/isos/v1/version` | `{api_schema_version,isos_version}` | 工作台兼容性检查。 |

进程管理器在启动前重新校验已保存图和精确版本的软件包。EOS 将 MuJoCo 作为直接子进程启动，并在停止获准的执行器后终止它。图布局属于编辑器；HTTP 图只包含运行配置和有类型连接。

### 按需核心配置适配器

IsOS 按需调用已配置的 `aether-profile setup --json` 或 `egia-config setup --json` 程序。标准输入为一个有界 JSON 对象：`{schema_version:1,op:"catalog"|"validate"|"render",core:"eos"|"egia",revision,graph,endpoints,packages}`。Run 时的 `revision` 是已保存图字节的 SHA-256。`graph` 是完整 v1 图，`endpoints` 包含解析后的 IsOS 与核心监听地址，`packages` 包含已校验精确包引用 `{id,version,path}`。适配器返回 `{schema_version:1,ok,diagnostics:[{path,code,message}],files:[{path,content}],catalog?}`。`render` 文件路径必须为以核心名称开头的相对路径，例如 `eos/graph.json`；文件内容为 UTF-8 文本。请求上限 1 MiB，响应上限 4 MiB，单个文件上限 1 MiB，配置超时 10 秒。适配器不绑定监听端口。

已启用的 EOS 进程节点通过 `settings.args` 提供 `--socket`；已启用的 EOS bridge 和 config 节点以 `127.0.0.1:<端口>` 提供 `--listen`。调用核心前，IsOS 会拒绝缺失、格式错误或重复的必需参数，不会静默改用 EOS 默认端点。当前保留的 EGIA 北向端点为 `127.0.0.1:16060`；实际监听由 EGIA 的受管启动配置负责，初始图不启用 EGIA 进程节点。核心 setup 必须支持 schema 1；缺少 setup 操作属于不兼容，不能回退到直接读取原始图。

Apply 校验已保存修订后生成已启用核心配置；Run 和 Restart 在启动子进程前复查修订与软件包输入。IsOS 再次检查已保存图的修订，并把接受的文件原子写入私有生成配置目录。修订过期时停止配置，且不替换图。EOS 生成的 `graph.json` 是已校验快照；当前 EOS 运行时仍直接读取已保存图。缺失或不兼容的 setup 适配器会阻止启动。MuJoCo 是外部 IsOS 包，核心配置不会构建它。

## 仅所有者可访问的管理器操作

Unix 套接字每行接收一个 JSON 请求：`{schema_version:1,op,deployment?,revision?,service_id?,cursor?,limit?}`，返回 `{schema_version:1,ok,data,error}`。管理器处理前检查对端 UID。`manager_identity` 返回 `pid`（进程 ID）、`start_ticks`（Linux 进程启动身份）、`mode`（`dev` 或 `product`）、`executable`（绝对路径）、`executable_sha256`（摘要）、IsOS/API/服务契约版本、`api_origin` 和 `ui_url`。用该身份区分已有的 IsOS 管理器与外部端口监听者；单凭端口号不能判定进程所有权。

`manager_shutdown` 停止所有图子进程和独立服务，然后结束 API、界面及管理器。`deployment_stop` 只影响指定部署。`services_list`、`services_validate`、`services_status`、`services_start`、`services_stop`、`services_restart` 和 `services_logs` 操作[独立服务定义](services_cn.md)；启动、停止、重启和日志请求需要 `service_id`，日志可带不透明的 `cursor` 及 1–200 的 `limit`。可执行文件摘要不同的 CLI 仍可请求关停，但不能启动或重启部署和服务。`isos status --json` 汇总部署状态、管理器身份和服务状态；`isos version check` 报告已配置核心、图和运行状态的就绪情况。

日志 `source` 可为 `all`、`isos`、`services`、`service-gateway` 等服务来源、`packages`、`graph`、`ui`、`eos`、`egia`、`mujoco`、`bridge`、`config` 或通过校验的子进程 ID。`limit` 范围为 1–200，默认为 100；省略 `source` 时使用 `[logging].default_source`（默认 `all`）。每行包含 RFC 3339 UTC `timestamp`、`level`、`source`、`message`，并可包含 `event_code` 和 `pid`。IsOS 记录自身诊断及选定的结构化子进程事件，不镜像普通标准输出或标准错误。即使当前文件已读完，`next_cursor` 仍为字符串，后续可用它继续轮询。日志目录权限为 `0700`、普通文件为 `0600`；达到 `[logging].max_bytes`（默认 256 KiB）后轮转，保留 `[logging].backups`（默认 2）份。日志源文件数量由 `[logging].max_sources` 限制（默认 32）；达到上限后，新来源只写入统一事件流，仍可按来源筛选。

`[logging].level` 按 `trace < debug < info < warn < error` 记录达到阈值的文件事件，默认为 `info`。HTTP 的 `level` 参数和 UI 选择器只过滤显示，不改变写入。终端状态和命令错误不受影响。`isos logs [source] --level warn` 读取同一日志库。

外部核心可通过仅所有者访问的管理套接字提交选定事件：`{schema_version:1,op:"log_append",deployment?,log:{source,level,message,event_code?}}`，响应含 `{accepted:true}`。`source` 为经校验的子进程 ID（1–48 个 ASCII 字母、数字、`_` 或 `-`），不能是 `all`、`events`、`services`、`isos` 或 `browser`；`level` 为五种级别之一；`message` 为 1–8192 个 UTF-8 字节；可选 `event_code` 为 1–64 个小写 ASCII 字母、数字或下划线。核心仍负责自己的完整日志。

应用图包含 MuJoCo 软件包时，IsOS 会把部署日志目录传给 EOS 运行时。只有 EOS 写入和轮转 `mujoco.log`，并遵守相同的 JSON 行与权限契约；IsOS 通过此 API 读取。

### 本地运行配置

IsOS 默认读取其源码目录中的 [config/isos.toml](../../../config/isos.toml)，与命令工作目录无关。`ISOS_CONFIG` 可指定必须存在的其他绝对配置文件路径。环境变量覆盖相应 TOML 字段；TOML 中的相对源码路径以 IsOS 源码目录为基准。私有部署状态仍使用 `ISOS_HOME` 或 XDG 目录。不会从命令工作目录搜索 `.env`。参见 [config/isos.example.toml](../../../config/isos.example.toml)。

| TOML 字段 | 类型／单位 | 用途与校验 | 环境变量覆盖 |
| --- | --- | --- | --- |
| `server.http_port` | 整数 TCP 端口 | IsOS 环回 API；1024–65535 | `ISOS_HTTP_PORT` |
| `server.ui_port` | 整数 TCP 端口 | IsosUI 环回前端；1024–65535，且不同于 HTTP 端口 | `ISOSUI_PORT` |
| `paths.ui_repo` | 路径字符串 | 可选 IsosUI 源码目录，仅供显式 `--ui-live` 使用；相对路径以 IsOS 源码目录为基准 | `ISOSUI_REPO_PATH` |
| `paths.ui_dist` | 路径字符串 | 含清单的预构建 IsosUI `dist/local` 目录；相对路径以 IsOS 源码目录为基准 | `ISOSUI_DIST_DIR` |
| `paths.aether_bin_dir` | 路径字符串 | 一套预构建 Aether Rust 程序的目录；相对路径以 IsOS 源码目录为基准 | `ISOS_AETHER_BIN_DIR` |
| `paths.egia_bin_dir` | 路径字符串 | 包含 `egia-config` 与 `egia` 的 EGIA 启动程序目录；相对路径以 IsOS 源码目录为基准 | `ISOS_EGIA_BIN_DIR` |
| `paths.mujoco_root` | 路径字符串 | 可选 MuJoCo 运行根目录；相对路径以 IsOS 源码目录为基准 | `MUJOCO_ROOT` |
| `deployment.name` | slug 字符串 | 默认图与日志命名空间；需通过部署名校验 | `ISOS_DEPLOYMENT` |
| `logging.max_bytes` | 整数，字节 | 轮转阈值 65,536–100,000,000；默认 262,144 | `ISOS_LOG_MAX_BYTES` |
| `logging.backups` | 整数，份数 | 轮转副本 1–10；默认 2 | `ISOS_LOG_BACKUPS` |
| `logging.default_source` | 来源字符串 | CLI 与 HTTP 日志默认来源；`all` 为统一流，默认 `all` | `ISOS_LOG_DEFAULT_SOURCE` |
| `logging.max_sources` | 整数，个数 | 来源文件上限 10–256，默认 32；超出后仍写统一流 | `ISOS_LOG_MAX_SOURCES` |
| `logging.level` | 枚举字符串 | 文件最低级别：`trace`、`debug`、`info`、`warn` 或 `error`；默认 `info` | `ISOS_LOG_LEVEL` |

## 已配置构建和版本就绪状态

在 EOS、EGIA 与 IsosUI 各自的仓库构建，再设置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 和 `paths.ui_dist`。IsOS 只解析已配置的产物。`isos version check` 默认输出可读的就绪报告；`isos version check --json` 为脚本返回同样状态。报告列出每个核心的程序路径、产品版本、setup 兼容性、图配置、运行状态及阻塞时的具体操作，并列出 IsOS 程序和界面文件路径。产品版本由核心报告；图修订与后端包版本是独立身份。

IsOS 在 `aether-profile setup --json` 或 `egia-config setup --json` 上发送 `{schema_version:1,op:"identity",core:"eos"|"egia"}` 探测核心。成功响应为 `{schema_version:1,ok:true,diagnostics:[],files:[],identity:{core,product_version,graph_version:"1.0.0",node_type_version:"1.0",setup_schema_version:1}}`。此操作无需图或端点，也不启动运行时。Run 和 Restart 在启动硬件前拒绝缺失或不兼容的程序，复查已保存修订与软件包输入，并在输入变化时重建已启用核心的配置。

请在 EOS、EGIA 和 IsosUI 所属仓库构建，再在 IsOS 中配置路径。重建已配置的核心后执行 `isos init`（或 `isos config init`），刷新已应用图的启动输入，再 Run 或显式 Restart。运行中的进程在重启前继续使用原有程序。尚未应用的图仍需 Save graph；`init` 不会暗中应用。IsOS 不根据 Git 提交推断选择。

`adapter_families` 包含 `id`（稳定适配器 ID）、`device_kinds`（设备类别）、可选精确版本 `package:{id,version}`、布尔值 `compiled`／`available`、字符串 `transport`／`description`、可选 `supported_primitives` 及不可用时的 `reason`。`primitive_bindings` 包含 `node_type`、`device_kind`、`movement_types`、`available`、`reason`、以及具有精确名称、版本、运动类型和描述的 `primitives`。`targets` 包含四段式 `path`、`node_type`、`device_kind`、`adapter_type`、`movement_types`、`available`、`reason`。`GET /node-types` 同时透传 EOS 的 `primitive_specs` 与有类型的 `data_types`。
