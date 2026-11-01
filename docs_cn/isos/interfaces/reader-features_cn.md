# P05 读取器与特性预检

[English](../../../docs/isos/interfaces/reader-features.md)

**P05 契约已实现；新增 R03 托管授权校验。**IsOS 在 Apply、Run 或 Restart 之前负责准入实际保存的部署图、精确软件包固定版本和已选核心程序。EOS、EGIA 各自负责程序发出的能力身份。本契约扩展[版本准入概览](configuration_cn.md#版本准入)和 schema-1 [本地 API](local-api_cn.md#已配置构建与版本就绪)。读取兼容不等于运行就绪：启用 EGIA 时，仍须通过 EOS 网关健康屏障。

## 调用、真实来源和探测

操作员调用 `isos version check --json`，或通过仅所有者可访问的 loopback Graph Validate/Apply 和部署 Run/Restart。IsOS 读取精确保存图字节及修订号。Validate 只检查提交的草稿；Apply 在图锁内、P04 发布前重查；Run/Restart 在停止或启动任何子进程前检查保存修订、所选 P04 生成版本及全部拟启动子进程。迁移预览检查建议的 `0.1.0` 图，但不会把只读旧版投影当成保存字节。探测均有界，且不启动核心运行时。

IsOS 从实际进程规格解析已配置的 `aether-profile` setup 读取器、EOS runtime、bridge、config-server，以及启用 EGIA 时所选的 EGIA setup 和 runtime。探测的程序字节必须与 P04 清单和随后启动的字节一致。能力探测须在 5 秒内退出，JSON 标准输出不超过 64 KiB，原始 stderr 不进入公开诊断。EGIA 的 `bin/egia` 是 shell 启动器：`EGIA_PYTHON`／`ISLAND_PYTHON`、`.venv` 回退、`ISLAND_PREFIX`、有效模块搜索路径及已安装 EGIA 模块代码均可能在启动器文件不变时改变 Python 进程。P05 将选定的绝对解释器路径、其解析后目标与摘要、有效的非密钥解释器／模块绑定和已验证的 EGIA 代码树纳入 P04 生成清单；探测与 spawn 使用同一条启动链，并在替换活动进程前重查。虚拟环境解释器的符号链接保留选定路径供 Python 定位环境，解析后的目标另行列入清单。解释器缺失时不得运行 `uv sync`、创建虚拟环境、安装包或做其他引导。仅路径相同、未选中的相邻构建、PATH 搜索、产品版本比较或探测另一个程序都不能准入子进程。探测、P04 生成版本核验和 spawn 之间若程序、解释器、模块树、绑定或包收据变化，预检失效；Run/Restart 在替换活动子进程前失败。EOS bridge 与 EGIA runtime 都不读取图，只检查各自必需的运行特性，不要求图／类型数组。

每个启用核心的 setup `identity` 保留 schema-1 的 `core`、`product_version`、`setup_schema_version`、旧 `graph_version` 和 `node_type_version`，并公布 `supported_graph_versions`、`supported_node_type_versions`、`supported_node_types` 和 `features`。数组给出精确接受版本及内置类型 ID；`features` 给出稳定的 ASCII 特性 ID。已验证包贡献另外核验，不能冒充内置 `supported_node_types`。旧标量是兼容别名，不得覆盖已有数组。读取图的 EOS 子进程通过有界的 `--capabilities` 报告 `binary`、`product_version`、支持数组及 `features`；EOS bridge 只报告现有身份与传输特性。EGIA P05 在所选启动器新增有界、不启动服务、不引导环境的 `egia --capabilities`；其 runtime 只报告 `binary`、`product_version` 和 `features`，因为它读取 P04 托管配置而非部署图。EGIA setup 负责图／类型数组和四个精确旧类型。IsOS 分别核验 setup 和 runtime。版本／类型数组仅部分存在、格式错误、为空、重复或矛盾时封闭失败。若旧 setup 身份的版本／类型数组全部缺失，只能将已校验的标量当成单元素集合，用于实际安全的 `1.0.0` 图、精确注册的 `@v1.*` 类型且推导出的 P05 setup 读取器必需特性集合为空；不得因此准入 `0.1.0`、`@v0.1` 或未在显式 `features` 中声明的特性。此 setup 回退不会绕过已启用 runtime 的特性探测。已有数组缺项时不得退回标量。

| 字段 | JSON 类型／单位 | 用途、缺省与校验 |
| --- | --- | --- |
| `supported_graph_versions` | 非空、唯一字符串数组／— | 程序实际读取的精确图版本；缺失时仅适用上述旧标量回退。 |
| `supported_node_type_versions` | 非空、唯一字符串数组／— | 精确类型后缀版本，如 `0.1`、`1.0`；必要但不足以准入类型 ID。 |
| `supported_node_types` | 唯一完整类型 ID 数组／— | 该读取器实现的精确内置类型；缺失时仅允许冻结的安全旧版注册表中的精确类型；贡献类型还要核验包 pin 和归属方目录。 |
| `features` | 唯一 ASCII 字符串数组／— | 程序实现的能力。缺失视为空集合；若有必需特性则失败。 |
| `binary` | 稳定程序 ID 字符串／— | runtime `--capabilities` 响应必需，须与所选角色一致，例如 `aether-runtime`；setup 用 `core`。 |
| `product_version` | 非空字符串／— | 仅供展示和来源核对，不是读取兼容谓词。 |
| `setup_schema_version` | 整数／— | setup 身份必须为 `1`；与图和产品版本独立。 |

EOS setup 夹具响应为 `{"schema_version":1,"ok":true,"diagnostics":[],"files":[],"identity":{"core":"eos","product_version":"fixture","graph_version":"1.0.0","node_type_version":"1.0","supported_graph_versions":["1.0.0","0.1.0"],"supported_node_type_versions":["1.0","0.1"],"supported_node_types":["eos.hardware.camera@v0.1"],"features":["setup-v1","graph-0.1-reader","catalog"],"setup_schema_version":1}}`。真实 EOS 程序的精确特性和全部内置类型由 EOS 同伴契约确定。EGIA 当前 setup 虽声明两种图版本，但只声明类型版本 `1.0`，不能单凭此准入 EGIA `@v0.1` 类型。针对 runtime 的夹具为 `{"binary":"aether-runtime","product_version":"fixture","graph_version":"1.0.0","node_type_version":"1.0","supported_graph_versions":["1.0.0","0.1.0"],"supported_node_type_versions":["1.0","0.1"],"supported_node_types":["eos.hardware.camera@v0.1"],"features":["graph-0.1-reader","log-dir"]}`。

## 必需集合与准入顺序

IsOS 从实际图读取版本，并在各 JSON 指针读取每个启用节点的完整注册类型 ID。先校验图结构和安全旧版身份，再验证每个精确包收据、贡献及固定版本，然后才能消费其类型目录。未知未来图仍使用 G01 的 `unsupported_graph_version`；未注册或未映射类型仍使用 G01／G03 的精确路径诊断。P05 的 `reader_graph_unsupported` 和 `reader_type_unsupported` 仅用于已知图版本或已注册类型不在所选读取器支持集合中的情形。对 EOS，先在不带 sidecar 或图输入的情况下探测 `--capabilities`，使响应仅描述内置能力。已有可重复的 `--node-types <absolute-path>` 参数只把已验证 sidecar 传给精确所选 `aether-profile` 的 setup／catalog／validation 调用及实际 EOS 启动；未带这些参数的 `setup catalog` 只有内置类型。IsOS 将贡献 ID 与组合目录及精确验证 pin 单独核验，不把它塞进静态能力响应。必需图／类型版本和特性 ID 来自组合后的归属方注册表、节点设置、连线与进程角色；图中自行声明的未知特性字符串不能授予能力。后缀匹配不能准入未注册类型，也不能准入来自不同 pin 的包贡献类型。MuJoCo 进程类型由已验证的 `island/mujoco-sim@0.1.1` 贡献；通用 EOS 相机、底盘、机械臂类型为内置。必需特性必须由实现它的精确程序满足。IsOS 依次检查启用的 setup 读取器、读取图的 runtime、非读取器传输子进程，最后核验 P04 生成输入、密钥和就绪前提。诊断顺序固定为保存图路径、程序角色、特性 ID；全图特性指向 `/version`。首个失败阻止 Apply/Run/Restart。包篡改保留 P01/P02 代码，优先于读取器不匹配；P04 生成快照不匹配优先于替换尝试，P05 不会隐式修复它。

对 `0.1.0` EOS 图，`aether-profile`、`aether-runtime` 和启用的 `aether-config-server` 均需要 `graph-0.1-reader`、`node-types`、`data-idl`。`aether-profile` 还需要 `setup-v1` 与 `catalog`；`aether-runtime` 还按启用节点和参数需要：IsOS 传入 `--log-dir` 时的 `log-dir`、原语节点的 `primitive-binding`、相机闸的 `vision-gate`，以及已准入 `eos.process.mujoco@v0.1` 或 `@v1.0` 启动的 `mujoco-child-v2`。EOS 自身对旧 `eos.process.backend@v1.0` 的 v2 子进程路径也要求此特性，但 IsOS G01／G03 会先以 `migration_required` 拒绝该旧图节点，所以 IsOS 不存在其 P05 runtime 特性用例。原有固定的 EOS `short-paths` 进程检查保持。只要启用 `aether-http-bridge` 进程，就需要其 `vision-ws` 传输特性，不要求它声明图读取能力。对 `0.1.0` 外壳，EGIA setup 需要 `setup-v1`、`catalog`、`graph-0.1-reader`、四个精确注册旧类型及其图／类型集合。所选 EGIA runtime 对任一可接受图外壳需要 `managed-config-v1` 与 `eos-gateway-client-v2`，不要求图／类型数组。这些只证明托管启动和客户端行为，不证明 EOS 网关就绪或未来 EGIA 阶段。即使读取器声明 `0.1.0`，缺少请求的完整注册类型或必需特性仍不兼容。

## 结构化失败与版本报告

R03 托管 `0.1.0` 启动还要求已选 `aether-runtime` 声明精确的 `managed-control-grants`。若缺失，则在签发授权或替换活动子进程前，以 `/cores/isos/layers/process/groups/component/nodes/eos/node_type` 路径报告 `reader_feature_unsupported`。现有 P05 有界 `--capabilities` 探测核验已选可执行文件的字节，旧 EOS 二进制不能忽略新验证文件而仍获执行器启动资格。

schema-1 HTTP 封装保留 `ok:false`、`error` 和 `data.diagnostics`；CLI 输出同一稳定代码及脱敏修复消息；`isos version check --json` 在每个核心现有的 `ready`、`reason`、`action` 字段旁包含同样的诊断对象。P05 每条诊断恰含 `code`、`path`、`message`、`stage` 和 `reader`。`reader` 含 `core`、`binary`、`binary_sha256`（小写 64 位十六进制；所选输入无法读取时为 `null`；对 EGIA，它是解析后的启动器／解释器／绑定／模块清单的规范摘要，不只是包装脚本）、`requested` 和 `supported`。`requested` 仅含适用的 `graph_version`、完整 `node_type`、`feature` 和精确 `{id,version}` 包 pin；不适用值为 `null`。`supported` 含排序且唯一的 `graph_versions`、`node_type_versions`、完整 `node_types`、`features`；未知集合为空数组。对 EGIA，`binary_sha256` 是紧凑、按键排序的 JSON 的 SHA-256；其中包含所选 `egia_chain` 对象，以及 `launchers` 中所选 `egia` 和 `egia-config` 文件的 SHA-256。全图要求指向 `/version`；节点或包要求指向精确的 `/node_type` 或 `/package`，并转义 JSON 指针。缺少运行时特性时指向提出要求的进程或设备 `/node_type`。所选子进程程序或启动链故障在图中有 `settings.role` 时指向该 IsOS 进程节点的 `/settings/role`，否则指向该节点 `/node_type`；命令路径由 IsOS 合成。setup 读取器使用 `""`。`stage` 为 `graph`、`package`、`reader` 或 `feature`。稳定代码为 `reader_identity_invalid`、`reader_graph_unsupported`、`reader_type_unsupported`、`reader_feature_unsupported`、`reader_binary_mismatch`、`reader_package_pin_mismatch`。`message` 点名所选程序及修复动作，不包含原始 stderr、密钥、本机路径或猜测的发行版。

例如缺少必需特性的读取器返回 HTTP `422`，`data` 为 `{"diagnostics":[{"code":"reader_feature_unsupported","path":"/cores/eos/layers/interface/groups/gate/nodes/preview/node_type","message":"aether-runtime lacks vision-gate; select a verified compatible build and Apply again","stage":"feature","reader":{"core":"eos","binary":"aether-runtime","binary_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","requested":{"graph_version":"0.1.0","node_type":"eos.interface.camera_gate@v0.1","feature":"vision-gate","package":null},"supported":{"graph_versions":["0.1.0","1.0.0"],"node_type_versions":["0.1","1.0"],"node_types":["eos.interface.camera_gate@v0.1"],"features":["log-dir"]}}}]}`，`error` 为 `"reader preflight failed"`。所选程序不可读取或角色不符时，spawn 前报告实际角色和 `reader_binary_mismatch`。身份格式错误时报告 `reader_identity_invalid`；若响应不可相信，受支持集合均为空。

`isos version check --json` 仅在 IsOS 已针对该图核验一个已安装或已索引发行版的签名／验证清单、精确资产摘要、核心角色、读取集合、必需特性及包贡献时，才可给出 `compatible_upgrade`。该字段为 `null` 或单个精确发行版 `{id,version,sha256}`；semver 范围、Git 分支、猜测 URL 或较新的产品版本均不够。列出候选不安装、不选择、不应用。缺少验证证据时，建议恢复兼容的所选构建或检查归属方发行目录。即使有候选，未来版本和未知版本依然失败。

[`tests/fixtures/reader_features/`](../../../tests/fixtures/reader_features/) 中的 P05 夹具冻结实现和跨仓库重放的正例、负例及不停止行为。其中 `reader_preflight` 操作仅隔离检查身份；R02 尚未提供 EOS 网关就绪时，`admit_reader_only` 不表示公开 Run 成功。所选读取器实现位于 `src/runtime/reader_features.rs`；EOS 与 EGIA 分别归属各自公布的身份。完整首次启动资格验证仍依赖 R02 网关。

## R04 host feature 目标

显式 EOS `settings.skill_libraries` 非空时，选中的 `aether-runtime` 必须通过有界 `--capabilities` 探测声明精确 `host-skill-abi-v2`。缺失时在 EOS component `/node_type` 返回同一 `reader_feature_unsupported`，且必须发生在 P04 发布或停止旧子进程之前；产品版本不能替代 feature。空列表或字段缺失不要求此 feature。详见[技能选择契约](skill-selection_cn.md)。
