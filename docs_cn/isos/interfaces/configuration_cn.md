# IsOS 配置、版本与日志契约

[English](../../../docs/isos/interfaces/configuration.md)

**状态：** XDG／`ISOS_HOME` 目录、`config/isos.toml`、schema-1 本地 API、按来源轮转的日志和 `isos version check` 已实现。P04 生成配置与 P05 所选读取器预检已在 IsOS 实现。跨核心事件字段仍是 **0.x 目标**。当前字段见[本地 API](local-api_cn.md#本地运行配置)，操作顺序见[首次使用](../usage/get-started_cn.md)。

## 目录与优先级

**R06 目标配置，尚未实现：** `deployment.stop_timeout_ms` 与 `deployment.eos_stop_grace_ms` 是拟议由管理器管理的停止预算；类型、单位、缺省值和跨归属方评审依赖见[依赖停止契约](dependent-stop_cn.md#停止期限及-eos-证明)。它们不是图字段；R06 解析器和校验落地前不要写入运行中的 `config/isos.toml`。

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

## P04 生成配置与启动预检（IsOS 已实现）

IsOS 归属已保存字节的修订、生成版本选择、私有文件和子进程启动。EOS 与 EGIA 归属各自 schema-1 `setup render` 适配器返回的内容；IsOS 不修改归属方覆盖文件。只有 Apply 写入；Validate 可在内存中渲染，但不发布。Run 和 Restart 只读取选中的生成版本，不暗中刷新。P05 单独协商实际读取器的功能和版本集合。

| 字段或路径 | 类型／单位 | 用途及校验 |
| --- | --- | --- |
| `revision` | 64 位小写十六进制字符／SHA-256 | 精确保存图字节的摘要，而非解析后 JSON；选择记录和清单均必需。 |
| `generation` | 64 位小写十六进制字符／SHA-256 | 规范、按字典序排序的输入清单摘要：修订、所选核心程序的路径与摘要、已验证软件包的 `{id,version,receipt_sha256}`、所选 EGIA 组件／工厂默认来源路径和文件摘要、最终渲染文件的路径与摘要。时钟、PID、密钥值及隐式工作目录搜索不进入清单。 |
| `selection.json` | JSON 对象／— | schema `2` 的按修订索引映射：`{"schema_version":2,"entries":{"<revision>":{"kind":"v2","generation":"<sha256>"}}}`。未标记的 P04 之前 G03 修订可显式记录为 `{"kind":"g03"}`。键是精确保存字节的 SHA-256。原子替换整个私有 `0600` 文件时保留当前及活动修订所需的旧条目。保存图摘要只选择自己的条目，其他修订的条目不能为它选择目录。 |
| `v2/required/<revision>.json` | 不可变 JSON 标记／— | 每个成功经 P04 Apply 的 `0.1.0` 修订发布 `{"schema_version":1,"revision":"<sha256>"}`，权限 `0600`，并 fsync 父目录。它与 `selection.json` 分离，重复 Apply 不删除。标记存在时，选择记录缺失、格式错误、为 `g03` 或 v2 无效，都阻止 Run／Restart；不得退回 G03 仅修订快照。首次 Apply 中断后若存在有效 v2 选择却缺标记，恢复流程先在图锁下验证目录并补齐标记，再允许 Run。只有未标记且无 v2 条目的修订才能使用已验证 G03 路径。 |
| `manifest.json` | JSON 对象／— | schema `2`，包含 `revision`、`generation`、已排序的 `files`、`launch_inputs`、`packages`、`default_roots`、`defaults` 与 `secret_refs`。每个文件条目包含相对 POSIX `path` 和小写 `sha256`；启动输入包含 `core`、`binary`、绝对 `path` 和 `sha256`；软件包含精确 `id`、`version` 及 `receipt_sha256`；未启用 EGIA 时 `default_roots` 为 `null`，启用时包含绝对的 `component` 和 `factory` 路径；默认文件包含 `source`（`component` 或 `factory`）、相对 `path` 和 `sha256`，未启用 EGIA 时为空数组。`secret_refs` 是排序后的 `{node_path,env_from}` 条目数组，未声明时为空。拒绝重复、未知、穿越或符号链接路径和字段。`files` 列出除 `manifest.json` 外末级目录的每个文件；不接受未列出的文件。清单及每个列出的普通文件为 `0700` 末级目录内的 `0600` 文件。 |
| `manifest.json.egia_chain` | JSON 对象／— | 仅启用 EGIA 时存在。`prefix` 是所选发行版绝对根目录；`mode` 为 `source` 或 `installed`；`interpreter` 是保留虚拟环境符号链接语义的所选绝对可执行路径；`interpreter_target` 为解析后的绝对目标；`interpreter_sha256` 为目标文件摘要。`binding` 是探测和子进程共用的已排序非密钥环境映射（`EGIA_PYTHON`、`ISLAND_PREFIX`、清空继承值的 `PYTHONPATH`、`PYTHONDONTWRITEBYTECODE`，安装模式还含 `ISLAND_LIB`／`ISLAND_LIBEXEC`）。`code` 是有效源码或已安装代码根目录下普通 EGIA 模块文件的有序 `{root,path,sha256}` 数组。缺失、变化、符号链接或不安全代码根目录、超过 4096 个文件或 64 MiB 均拒绝；Run／Restart 前比较整个对象。该字段参与 `generation`，未启用 EGIA 时省略。 |
| `eos/graph.json` | JSON 字节／— | 必需的 EOS 归属方渲染；Run／Restart 前其摘要和精确字节须与重新渲染一致。EOS 有效 `--profile` 指向这里；人工编辑图仍位于 `config/`。 |
| `egia/` | TOML 字节／— | 启用 EGIA 进程时必需。先放入完整的已选已安装 EGIA `etc/egia` 默认目录，再用 EGIA 校验后渲染的 `egia/egia.toml`、`egia/modes.toml` 和 `egia/profiles/isos.toml` 精确替换。必需的有效文件为 `egia.toml`、`modes.toml`、`profiles/isos.toml`、`models.toml`、`prompts.toml`、`missions.toml`、`safety/gate.toml` 和 `opt/sandbox.toml`；其他声明过的普通默认文件也复制。拒绝缺失的必需文件、符号链接、特殊文件、冲突、逃逸引用和不受支持的源布局。启动时不得由未列出的工厂回退目录补齐缺失值。 |
| `secret_refs` | 节点路径及子进程到管理器环境变量名称映射的数组／— | 部署条目只从目标组件 `settings.env_from` 导出；已有独立服务的 `env_from` 在本清单外遵循同样的 spawn 规则。两种名称均须符合 `[A-Za-z_][A-Za-z0-9_]*`。清单只存名称。停止任何活动子进程**之前**，为所有计划启动的子进程解析并检查每个必需值，放入私有、短生命周期启动计划；缺失或空值拒绝启动且不打印。仅在对应子进程 spawn 时传入其值，随后清除该子进程的副本。图、清单、日志、状态及诊断都不序列化值。原始图 `settings.env` 不解释为密钥引用机制。 |

已安装 EGIA 默认目录是配置好的 EGIA 发行版中的显式 `paths.egia_defaults_dir` 和 `paths.egia_factory_defaults_dir` 输入，不通过工作目录搜索。启用 EGIA 时两者均必需；IsOS 须确认工厂路径就是所选 EGIA 启动程序实际读取的根目录。IsOS 在不跟随符号链接的情况下检查所选 `egia-config`、`egia` 程序及每个默认文件均为普通文件，记录 SHA-256，并拒绝渲染到发布期间变化的来源。组件默认文件复制到末级目录；工厂默认文件仍作为清单中的启动输入，在 Run／Restart 和 spawn 前再次核验。未列出的工厂回退目录会使预检失败。P01 校验器只提供精确固定版本的软件包句柄；软件包侧车或未验证目录不能变成核心默认配置。EOS 配置和 EGIA 配置来自同一个保存图修订与一组已验证固定版本。`EGIA_ETC` 与显式 `--etc` 参数都指向选中末级目录下绝对的 `egia/`；托管启动仍使用 `--mode isos --managed --no-tui`。不得从隐式源码仓库文件补齐缺失的有效值。

计算生成摘要时，取除 `generation` 成员外的完整清单对象，编码为紧凑 UTF-8 JSON，对象键按字典序排序，不含无意义空白或末尾换行，然后对这些字节计算 SHA-256。编码前，`files` 按 `path`、`launch_inputs` 按 `(core,binary)`、`packages` 按 `(id,version)`、`defaults` 按 `(source,path)`、`secret_refs` 按 `node_path` 排序；每个 `env_from` 映射按子进程名称排序。根目录与程序路径使用选定的绝对拼写，文件使用规范的相对 POSIX 拼写。校验器重算摘要；格式正确但生成摘要错误的清单会被拒绝。

Apply 在最终图锁下重新检查预期的保存图字节修订、软件包句柄、程序／默认文件摘要及归属方渲染。先暂存并 fsync 不可变末级目录，再原子发布尚不存在的目录并 fsync 父目录。随后构建完整 schema-2 选择映射，保留当前和活动修订的旧条目（未标记的 G03 快照显式记为 `g03`），加入或替换候选修订的 v2 生成版本；写入并 fsync 临时映射，重命名为 `selection.json`，再 fsync 选择文件父目录。该修订首次 P04 Apply 时，随后将标记写入私有临时文件并 fsync，原子重命名为 `v2/required/<revision>.json`，再 fsync 父目录；已有标记不重写。同修订 v2 重复 Apply 在选择映射重命名与父目录 fsync 后完成：标记已存在，图字节不变。G03 同修订首次升级须等标记持久化才完成。新建或变更图时，标记和映射须在暂存图文件原子重命名及其父目录 fsync **之前**持久化。图重命名是保存字节的发布点。Apply 不改变活动子进程；相同图字节的新生成版本须显式 Apply，旧目录保持不可变。

选择映射重命名前失败，旧保存图与旧映射不变。映射重命名后，变更图仍选择保留的旧条目；同修订图可能选择完整的新旧任一生成版本；活动子进程不变。标记发布失败后映射仍可观察：恢复时对当前修订有效 v2 条目补齐标记，旧图保存期间未来修订条目不生效。图重命名前旧保存图仍在；重命名后只接受实际观察到的新旧完整状态。选择、标记或图任一重命名后、父目录 fsync 前失败均返回 `generated_commit_uncertain`、`stage:"publish"`，不能声称旧状态完好。恢复流程在图锁下重新读取精确图字节、选择映射、标记及完整选中目录。图仍为旧修订时使用保留的旧条目，未来条目不生效；图为新修订时，须验证新标记与 v2 条目后才允许 Run／Restart。首次同修订升级中断，若有有效 v2 条目但缺标记，验证目录后补发标记；否则关闭准入。结果不确定的重命名再次 fsync 相应父目录，并以观察到的保存字节修订和生成版本判定结果。成功 P04 Apply 后即使映射丢失，独立标记也禁止 G03 回退。只有保存及活动修订都不再引用的目录和映射条目才可回收。

Run 和 Restart 在部署锁下重新读取保存字节、匹配的选择条目及独立标记，校验 P01 精确软件包句柄及完整清单／文件集合，从当前已安装输入重算生成标识，并逐字节比较归属方重新渲染。文件和清单摘要一起被修改，仍会在归属方比较时失败。停止任何活动子进程之前，为**所有**计划启动的子进程（包括依赖顺序靠后的子进程）解析**全部**必需密钥名称；缺失值以 `generated_missing_secret`、`stage:"spawn"` 报告精确转义后的 `/cores/isos/layers/process/groups/component/nodes/<node>/settings/env_from/<target>` JSON 指针。值只在私有启动计划中保存，并在各自子进程 spawn 时传入。所有预检须在停止或替换活动子进程前完成；失败不改变活动状态。`1.0.0` 保存图在显式迁移前保留原有平面生成快照和安全旧版准入；P04 不改写这些图字节。

P04 失败使用已有 schema-1 本地 API 封装：准入失败用 HTTP `422`，`generated_commit_uncertain` 用 `500`，并返回有界的 `data.diagnostics` 数组。每条诊断恰含 `code`、`path`（JSON 指针，全局输入用空字符串）、`message` 与 `stage`；消息给出修复动作，不暴露本机来源路径、适配器原始 stderr、TOML 内容、密钥名称或值。稳定错误码为 `generated_revision_mismatch`、`generated_missing_input`、`generated_unsafe_path`、`generated_invalid_default`、`generated_owner_rejected`、`generated_manifest_mismatch`、`generated_render_mismatch`、`generated_missing_secret` 和 `generated_commit_uncertain`。阶段仅为 `graph`、`package`、`binary`、`defaults`、`render`、`publish`、`snapshot` 或 `spawn`。节点错误指向保存图的精确路径；全局文件错误用 `""`。例如 EGIA 提示文件缺失时为 `{ "code":"generated_missing_input", "path":"", "message":"Restore the selected EGIA installation and Apply again", "stage":"defaults" }`。已标记修订的选择记录缺失或损坏时，用 `generated_manifest_mismatch`、`stage:"snapshot"`；选择记录没有匹配的保存修订时，用 `generated_revision_mismatch`、`stage:"graph"`。缺失子进程密钥指向精确的 `settings.env_from` 目标键，对每个节点和键片段做 JSON Pointer 转义，诊断消息不重复来源名称。CLI 返回非零状态及相同错误码和脱敏消息。选择映射重命名前拒绝不会激活候选；之后的发布故障及不确定结果须按观察到的图字节、映射、标记与目录恢复。

## 版本准入

| 身份 | 目标值或规则 | 兼容含义 |
| --- | --- | --- |
| 产品发行 | IsOS／EGIA／IsosUI `0.1.0`、EOS `0.2.0`、MuJoCo 包 `0.1.1` | 没有产品宣称稳定版 `1.0.0`。 |
| 新图写入格式 | `0.1.0` | 读取器也接受已有 `1.0.0` 图；原始字节不变。 |
| 新节点类型 ID | `@v0.1` | 旧 `@v1.0`／`@v1.1` 保持已注册的安全行为。 |
| HTTP 路由和 setup 封装 | `/api/isos/v1`、`schema_version:1` | 兼容身份，不是产品版本。 |
| 包发行清单 | `schema_version:4` | 分发格式；精确包版本另计。 |
| MuJoCo 套接字协议 | `2.0.0` | 已有仿真器连接身份；EOS 与包必须一致。 |

**P05 读取准入：**[读取器特性契约](reader-features_cn.md)规定精确的所选程序探测、完整类型与已验证包检查、必需特性映射、旧标量回退、结构化诊断及升级证据。Validate 和 Apply 检查草稿；Run 和 Restart 在替换子进程前检查保存字节及所选 P04 生成版本。包、图、程序或 EGIA 启动链变化使所选生成版本失效。安全的 `1.0.0` 图在显式迁移前保留原始字节。没有与图精确匹配的已验证发行版就不建议兼容升级。

## 共享 JSONL 事件

[P06 共享事件契约](events_cn.md)持有精确信封、管理器会话与部署运行 ID、包来源、子进程接收、脱敏及高频抑制。IsOS 保留私有轮转分类文件和统一 `events.log`；[本地日志 API](local-api_cn.md)保留现有游标与过滤接口。实时就绪以 status 为准。

## R04 生成技能选择目标

非空 EOS component `settings.skill_libraries` 列表使 IsOS 在 P04 leaf 中生成 `isos/skill-selection.json`，并在 `manifest.json.files` 列出其 SHA-256。仅选中的 EOS 子进程通过 `EOS_SKILL_SELECTION_FILE` 接收绝对路径。保存图 revision、精确包 pin、库摘要与 generation 均须在停止旧子进程前重新验证；详见[选择契约](skill-selection_cn.md)。
