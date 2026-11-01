# IsOS 配置、版本与日志契约

[English](../../../docs/isos/interfaces/configuration.md)

**状态：** XDG／`ISOS_HOME` 目录、`config/isos.toml`、schema-1 本地 API、按来源轮转的日志和 `isos version check` 已实现。下文 P04 生成配置契约、跨核心事件字段及受支持版本数组是 **0.x 目标**。当前字段见[本地 API](local-api_cn.md#本地运行配置)，操作顺序见[首次使用](../usage/get-started_cn.md)。

## 目录与优先级

`ISOS_HOME` 必须为绝对路径；设置后根目录为 `<base>/config`、`<base>/data`、`<base>/run`。否则使用 `${XDG_CONFIG_HOME:-~/.config}/isos`、`${XDG_DATA_HOME:-~/.local/share}/isos` 与 `${XDG_RUNTIME_DIR:-/run/user/<uid>}/isos`。IsOS 使用仅所有者可访问的目录（`0700`）和私有文件（`0600`），不把命令工作目录当作配置目录。

| 相对路径／所有者 | 类型与用途 | 写入和校验规则 |
| --- | --- | --- |
| `config/deployments/<name>.nodes.json`／IsOS | 人工编写的部署图 | 名称通过 slug 校验；修订为已保存字节的 SHA-256；只有修订检查后的 Apply 或显式迁移可写。 |
| `config/services/<id>.toml`／IsOS | 独立启动的模型类服务 | 只能通过显式服务命令启动；本次不作为图节点。 |
| `config/mode`／IsOS | `dev` 或 `product` | `isos init` 写入；无效值阻止启动。 |
| `data/generated/<name>/v2/<revision>/<generation>/`／IsOS | P04 已校验的核心配置快照 | **目标：**通过修订检查后的 `data/generated/<name>/selection.json` 选择不可变末级目录；详见下文契约。现有 `data/generated/<name>/<revision>/` 和旧版平面快照仍可读取，但不改写。 |
| `data/packages/<namespace>/<name>/<version>/`／IsOS | 精确版本包、收据和节点类型侧车 | 安装或链接只产生收据，不激活图；Apply 与 Run 重新计算链接内容指纹。 |
| `data/logs/<name>/`／IsOS 与 EOS 子进程 | 按来源轮转的 JSONL 和统一 `events.log` | 仅所有者访问；有界游标读取；目标事件含运行 ID。 |
| `data/cache/`／IsOS | 下载和目录缓存 | **目标：**可删除；包身份依赖收据和摘要，不依赖缓存存在。 |
| `run/manager.sock`、`run/<name>-*.sock`／所属进程 | 私有管理器与核心套接字 | 检查 UID；确认旧所有者身份后才清理遗留套接字。 |

设置 `ISOS_CONFIG` 时它指定必须存在的绝对 TOML 文件，否则读取 IsOS 仓库的 `config/isos.toml`。每个字段依次采用显式环境变量、TOML、文档默认值。TOML 中的相对源码产物路径以 IsOS 仓库为基准；`ISOS_HOME`／XDG 独立选择私有状态根目录。不在图设置、生成快照、收据、日志或 UI 偏好中存放密钥。服务 `env_from` 在启动时导入命名环境变量；状态只显示名称，不显示值。缺少必需密钥时预检失败，且不回显值。

## P04 生成配置与启动预检（目标）

IsOS 归属已保存字节的修订、生成版本选择、私有文件和子进程启动。EOS 与 EGIA 归属各自 schema-1 `setup render` 适配器返回的内容；IsOS 不修改归属方覆盖文件。只有 Apply 写入；Validate 可在内存中渲染，但不发布。Run 和 Restart 只读取选中的生成版本，不暗中刷新。P05 单独协商实际读取器的功能和版本集合。

| 字段或路径 | 类型／单位 | 用途及校验 |
| --- | --- | --- |
| `revision` | 64 位小写十六进制字符／SHA-256 | 精确保存图字节的摘要，而非解析后 JSON；选择记录和清单均必需。 |
| `generation` | 64 位小写十六进制字符／SHA-256 | 规范、按字典序排序的输入清单摘要：修订、所选核心程序的路径与摘要、已验证软件包的 `{id,version,receipt_sha256}`、所选 EGIA 组件／工厂默认来源路径和文件摘要、最终渲染文件的路径与摘要。时钟、PID、密钥值及隐式工作目录搜索不进入清单。 |
| `selection.json` | JSON 对象／— | `{"schema_version":1,"revision":"<sha256>","generation":"<sha256>"}`。IsOS 只在取得图锁并成功通过修订检查的 Apply 后原子替换。文件权限为 `0600`；对 P04 已应用图，缺失、格式错误或不匹配会阻止 Run／Restart。尚无选择记录的 P04 之前 G03 仅修订快照在显式 Apply 升级前仍按现有已校验读取路径使用。 |
| `manifest.json` | JSON 对象／— | schema `2`，包含 `revision`、`generation`、已排序的 `files`、`launch_inputs`、`packages`、`default_roots`、`defaults` 与 `secret_refs`。每个文件条目包含相对 POSIX `path` 和小写 `sha256`；启动输入包含 `core`、`binary`、绝对 `path` 和 `sha256`；软件包含精确 `id`、`version` 及 `receipt_sha256`；未启用 EGIA 时 `default_roots` 为 `null`，启用时包含绝对的 `component` 和 `factory` 路径；默认文件包含 `source`（`component` 或 `factory`）、相对 `path` 和 `sha256`，未启用 EGIA 时为空数组。`secret_refs` 是排序后的 `{node_path,env_from}` 条目数组，未声明时为空。拒绝重复、未知、穿越或符号链接路径和字段。`files` 列出除 `manifest.json` 外末级目录的每个文件；不接受未列出的文件。清单及每个列出的普通文件为 `0700` 末级目录内的 `0600` 文件。 |
| `eos/graph.json` | JSON 字节／— | 必需的 EOS 归属方渲染；Run／Restart 前其摘要和精确字节须与重新渲染一致。EOS 有效 `--profile` 指向这里；人工编辑图仍位于 `config/`。 |
| `egia/` | TOML 字节／— | 启用 EGIA 进程时必需。先放入完整的已选已安装 EGIA `etc/egia` 默认目录，再用 EGIA 校验后渲染的 `egia/egia.toml`、`egia/modes.toml` 和 `egia/profiles/isos.toml` 精确替换。必需的有效文件为 `egia.toml`、`modes.toml`、`profiles/isos.toml`、`models.toml`、`prompts.toml`、`missions.toml`、`safety/gate.toml` 和 `opt/sandbox.toml`；其他声明过的普通默认文件也复制。拒绝缺失的必需文件、符号链接、特殊文件、冲突、逃逸引用和不受支持的源布局。启动时不得由未列出的工厂回退目录补齐缺失值。 |
| `secret_refs` | 节点路径及子进程到管理器环境变量名称映射的数组／— | 部署条目只从目标组件 `settings.env_from` 导出；已有独立服务的 `env_from` 在本清单外遵循同样的 spawn 规则。两种名称均须符合 `[A-Za-z_][A-Za-z0-9_]*`。清单只存名称。紧邻该子进程 spawn 时从 IsOS 管理器环境解析值，仅传给该子进程；缺失或空的必需值拒绝启动，不打印。原始图 `settings.env` 不解释为密钥引用机制。 |

已安装 EGIA 默认目录是配置好的 EGIA 发行版中的显式 `paths.egia_defaults_dir` 和 `paths.egia_factory_defaults_dir` 输入，不通过工作目录搜索。启用 EGIA 时两者均必需；IsOS 须确认工厂路径就是所选 EGIA 启动程序实际读取的根目录。IsOS 在不跟随符号链接的情况下检查所选 `egia-config`、`egia` 程序及每个默认文件均为普通文件，记录 SHA-256，并拒绝渲染到发布期间变化的来源。组件默认文件复制到末级目录；工厂默认文件仍作为清单中的启动输入，在 Run／Restart 和 spawn 前再次核验。未列出的工厂回退目录会使预检失败。P01 校验器只提供精确固定版本的软件包句柄；软件包侧车或未验证目录不能变成核心默认配置。EOS 配置和 EGIA 配置来自同一个保存图修订与一组已验证固定版本。`EGIA_ETC` 与显式 `--etc` 参数都指向选中末级目录下绝对的 `egia/`；托管启动仍使用 `--mode isos --managed --no-tui`。不得从隐式源码仓库文件补齐缺失的有效值。

计算生成摘要时，取除 `generation` 成员外的完整清单对象，编码为紧凑 UTF-8 JSON，对象键按字典序排序，不含无意义空白或末尾换行，然后对这些字节计算 SHA-256。编码前，`files` 按 `path`、`launch_inputs` 按 `(core,binary)`、`packages` 按 `(id,version)`、`defaults` 按 `(source,path)`、`secret_refs` 按 `node_path` 排序；每个 `env_from` 映射按子进程名称排序。根目录与程序路径使用选定的绝对拼写，文件使用规范的相对 POSIX 拼写。校验器重算摘要；格式正确但生成摘要错误的清单会被拒绝。

Apply 在最终发布锁下重新检查保存图字节修订、软件包句柄、程序与默认文件摘要及归属方渲染。先建立仅所有者可访问的同级暂存末级目录，写入并 fsync 每个文件和清单、fsync 暂存目录，然后原子发布尚不存在的生成末级目录，最后原子发布并 fsync `selection.json`。相同输入再次 Apply 选择同一生成版本，不改写。程序或默认文件变化但图字节未变时，只有显式 Apply 才能创建另一个生成版本；旧目录保持不可变且不覆盖。选择记录发布前失败时，保持先前选择、图字节、活动修订和子进程不变；发布后失败属于结果不确定，须重新读取选择记录及完整目录，再报告成功或失败。单纯 Apply 从不改变活动子进程。

Run 和 Restart 在部署锁下重新读取保存字节和选择记录，校验 P01 精确软件包句柄及完整清单／文件集合，从当前已安装输入重算生成标识，并逐字节比较归属方重新渲染。文件和清单摘要一起被修改，仍会在归属方比较时失败。只在各自目标子进程 spawn 时解析必需密钥名称。所有检查须在停止或替换活动子进程之前完成；失败不改变其活动状态。`1.0.0` 保存图在显式迁移前保留原有平面生成快照和安全旧版准入；P04 不改写这些图字节。

P04 失败使用已有 schema-1 本地 API 封装、HTTP `422` 和有界的 `data.diagnostics` 数组。每条诊断恰含 `code`、`path`（JSON 指针，全局输入用空字符串）、`message` 与 `stage`；消息给出修复动作，不暴露本机来源路径、适配器原始 stderr、TOML 内容、密钥名称或值。稳定错误码为 `generated_revision_mismatch`、`generated_missing_input`、`generated_unsafe_path`、`generated_invalid_default`、`generated_owner_rejected`、`generated_manifest_mismatch`、`generated_render_mismatch` 和 `generated_missing_secret`。阶段仅为 `graph`、`package`、`binary`、`defaults`、`render`、`publish`、`snapshot` 或 `spawn`。节点错误指向保存图的精确路径；全局文件错误用 `""`。例如 EGIA 提示文件缺失时为 `{ "code":"generated_missing_input", "path":"", "message":"Restore the selected EGIA installation and Apply again", "stage":"defaults" }`。CLI 返回非零状态及相同错误码和脱敏消息。拒绝时不会激活候选目录或选择记录。

## 版本准入

| 身份 | 目标值或规则 | 兼容含义 |
| --- | --- | --- |
| 产品发行 | IsOS／EGIA／IsosUI `0.1.0`、EOS `0.2.0`、MuJoCo 包 `0.1.1` | 没有产品宣称稳定版 `1.0.0`。 |
| 新图写入格式 | `0.1.0` | 读取器也接受已有 `1.0.0` 图；原始字节不变。 |
| 新节点类型 ID | `@v0.1` | 旧 `@v1.0`／`@v1.1` 保持已注册的安全行为。 |
| HTTP 路由和 setup 封装 | `/api/isos/v1`、`schema_version:1` | 兼容身份，不是产品版本。 |
| 包发行清单 | `schema_version:4` | 分发格式；精确包版本另计。 |
| MuJoCo 套接字协议 | `2.0.0` | 已有仿真器连接身份；EOS 与包必须一致。 |

**目标算法：**不启动核心，先探测每个已启用核心的 setup `identity`；读取 `supported_graph_versions`、`supported_node_type_versions`、setup schema 与产品版本；与实际保存图和精确包贡献逐一比较。校验包收据／摘要及已编译适配器。`isos version check --json` 对每个不匹配项返回受支持列表和可执行操作。Apply 与 Run 针对当前字节和程序重复校验；包、图或程序变化使生成快照失效。不可把 `1.0.0` 图伪装为 `0.1.0` 以通过校验。当前 identity 只有 `graph_version` 与 `node_type_version` 单个字符串，数组是目标扩展。

## 目标统一 JSONL 事件

模块在首次就绪时写一条 `info`，随后只记录状态变化、警告和错误；周期健康检查只在变化时或有界汇总间隔记录。完整子进程输出由其所有者保存，IsOS 统一流只接收有界、已脱敏事件。日志写入不得阻塞执行器停止或进程退出。

| 字段 | JSON 类型／单位 | 用途、默认值、校验与兼容 |
| --- | --- | --- |
| `timestamp` | RFC 3339 UTC 字符串／毫秒 | 必需事件时间；保留已有字段。 |
| `level` | `trace`、`debug`、`info`、`warn`、`error` | 必需级别；保留现有过滤顺序。 |
| `source` | 字符串／来源 ID | 必需，1–48 个 ASCII 字母、数字、`_` 或 `-`。 |
| `core` | 字符串／核心 ID | 必需运行时所有者（`isos`、`eos` 或 `egia`）；MuJoCo 子进程事件使用 `eos`，`source:mujoco`。 |
| `node_path` | 字符串或 null／规范路径 | 可选所属节点；全局事件为 null 或省略。 |
| `event_code` | 字符串／机器码 | 必需稳定 snake_case，1–64 字符；旧读取器仍接受缺省。 |
| `run_id` | 字符串／不透明 ID | 必需所有者会话身份；部署前用管理器会话 ID，部署后用部署运行 ID；旧 `log_append` 规范化时由 IsOS 补齐。 |
| `correlation_id` | 字符串或 null／不透明 ID | 可选请求关联，不嵌入密钥或用户文本。 |
| `message` | 字符串／UTF-8 字节 | 必需人类说明，最多 8192 字节，写前脱敏。 |
| `fields` | 对象／有界元数据 | 可选，只含标量，最多 16 键和 4 KiB 编码大小；拒绝密钥、含凭证路径及无界负载。 |
| `pid` | 整数或 null／进程 ID | 可选已有进程元数据。 |

```json
{"timestamp":"2026-11-01T10:00:00.000Z","level":"info","source":"eos","core":"eos","node_path":null,"event_code":"runtime_ready","run_id":"r-7c2a","correlation_id":null,"message":"EOS runtime ready","fields":{"state":"ready"},"pid":4242}
```

用 `isos logs all --level info` 或 `GET /api/isos/v1/logs?source=all&limit=100` 读取，随后用 `next_cursor` 轮询。每次读取最多 200 行或 64 KiB；`rotated:true` 表示游标对应文件已轮转，UI 要显示空档。当前默认值为每文件 256 KiB、两份备份、32 个来源文件、最低 `info`。超过来源文件上限的新来源仍写统一流。就绪日志不代表实时健康；`isos status --json` 才是当前状态依据。

**目标接收算法：**校验来源、级别、长度；附上经过认证的所有者和运行上下文；删除密钥字段与凭证样式值；限制字段；在锁内向来源和统一流写一条 JSON 行并轮转；慢速 UI 读取不得阻塞。丢弃／拒绝事件增加可观察计数，警告限速以防递归。HTTP 与 CLI 在存储后过滤。已有 `log_append` 仍是仅所有者可访问的兼容入口；未来跨核心传输可新增字段，不要求旧调用方发送。
