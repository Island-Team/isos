# P05 读取器与特性预检

[English](../../../docs/isos/interfaces/reader-features.md)

**目标设计，尚未实现。**IsOS 在 Apply、Run 或 Restart 之前负责准入实际保存的部署图、精确软件包固定版本和已选核心程序。EOS、EGIA 各自负责程序发出的能力身份。本契约扩展[版本准入概览](configuration_cn.md#版本准入)和 schema-1 [本地 API](local-api_cn.md#已配置构建与版本就绪)。读取兼容不等于运行就绪：启用 EGIA 时，仍须通过 EOS 网关健康屏障。

## 调用、真实来源和探测

操作员调用 `isos version check --json`，或通过仅所有者可访问的 loopback Graph Validate/Apply 和部署 Run/Restart。IsOS 读取精确保存图字节及修订号。Validate 只检查提交的草稿；Apply 在图锁内、P04 发布前重查；Run/Restart 在停止或启动任何子进程前检查保存修订、所选 P04 生成版本及全部拟启动子进程。迁移预览检查建议的 `0.1.0` 图，但不会把只读旧版投影当成保存字节。探测均有界，且不启动核心运行时。

IsOS 从实际进程规格解析已配置的 `aether-profile` setup 读取器、EOS runtime、bridge、config-server，以及启用 EGIA 时所选的 EGIA setup 和 runtime。探测的程序路径及摘要必须与 P04 清单和随后启动的程序一致。仅路径相同、未选中的相邻构建、PATH 搜索、产品版本比较或探测另一个程序都不能准入子进程。探测、P04 生成版本核验和 spawn 之间若程序或包收据变化，预检失效；Run/Restart 在替换活动子进程前失败。bridge 不读取图，因此检查其旧版标量能力身份和必需传输特性，不要求图／类型数组。

每个启用核心的 setup `identity` 保留 schema-1 的 `core`、`product_version`、`setup_schema_version`、旧 `graph_version` 和 `node_type_version`，并公布 `supported_graph_versions`、`supported_node_type_versions`、`supported_node_types` 和 `features`。数组给出精确接受版本及内置类型 ID；`features` 给出稳定的 ASCII 特性 ID。已验证包贡献另外核验，不能冒充内置 `supported_node_types`。旧标量是兼容别名，不得覆盖已有数组。所选 runtime 在读取图时须报告 `binary`、`product_version`、相同的受支持数组和 `features`；EOS 使用 `--capabilities`，EGIA P05 在所选启动器新增有界、不启动服务的 `egia --capabilities`。IsOS 分别核验 setup 和 runtime。版本／类型数组仅部分存在、格式错误、为空、重复或矛盾时封闭失败。若旧身份的版本／类型数组全部缺失，只能将已校验的标量当成单元素集合，用于实际安全的 `1.0.0` 图和精确注册的 `@v1.*` 类型；不得因此准入 `0.1.0`、`@v0.1` 或未在显式 `features` 中声明的特性。已有数组缺项时不得退回标量。

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

IsOS 从实际图读取版本，并在各 JSON 指针读取每个启用节点的完整注册类型 ID。先校验图结构和安全旧版身份，再验证每个精确包收据、贡献及固定版本，然后才能消费其类型目录。未知未来图仍使用 G01 的 `unsupported_graph_version`；未注册或未映射类型仍使用 G01／G03 的精确路径诊断。P05 的 `reader_graph_unsupported` 和 `reader_type_unsupported` 仅用于已知图版本或已注册类型不在所选读取器支持集合中的情形。对 EOS，使用现有可重复的 `--node-types <absolute-path>` 参数，把每个已验证 sidecar 传给精确所选 `aether-profile` 的 setup／catalog 探测及 EOS runtime／config-server 探测；未带这些参数的 `setup catalog` 只有内置类型。必需图／类型版本和特性 ID 来自组合后的归属方注册表、节点设置、连线与进程角色；图中自行声明的未知特性字符串不能授予能力。后缀匹配不能准入未注册类型，也不能准入来自不同 pin 的包贡献类型。MuJoCo 进程类型由已验证的 `island/mujoco-sim@0.1.1` 贡献；通用 EOS 相机、底盘、机械臂类型为内置。必需特性必须由实现它的精确程序满足。IsOS 依次检查启用的 setup 读取器、读取图的 runtime、非读取器传输子进程，最后核验 P04 生成输入、密钥和就绪前提。诊断顺序固定为保存图路径、程序角色、特性 ID；首个失败阻止 Apply/Run/Restart。包篡改保留 P01/P02 代码，优先于读取器不匹配；P04 生成快照不匹配优先于替换尝试，P05 不会隐式修复它。

对 `0.1.0` EOS 图，`aether-profile`、`aether-runtime` 和启用的 `aether-config-server` 均需要 `graph-0.1-reader`、`node-types`、`data-idl`。`aether-profile` 还需要 `setup-v1` 与 `catalog`；`aether-runtime` 还按启用节点和参数需要：IsOS 传入 `--log-dir` 时的 `log-dir`、原语节点的 `primitive-binding`、相机闸的 `vision-gate`，以及 `eos.process.mujoco@v0.1`、`eos.process.mujoco@v1.0` 或通过 `eos.process.backend@v1.0` 启动已验证 v2 子进程时的 `mujoco-child-v2`。原有固定的 EOS `short-paths` 进程检查保持。只要启用 `aether-http-bridge` 进程，就需要其 `vision-ws` 传输特性，不要求它声明图读取能力。EGIA setup/runtime 的要求只来自精确 EGIA 注册类型及其归属方特性声明；在 EGIA 提供可核验的 runtime 身份之前，启用 EGIA 的进程保持阻塞。即使读取器声明 `0.1.0`，缺少请求的完整 `@v0.1` 类型或必需特性仍不兼容。

## 结构化失败与版本报告

schema-1 HTTP 封装保留 `ok:false`、`error` 和 `data.diagnostics`；CLI 输出同一稳定代码及脱敏修复消息；`isos version check --json` 在每个核心现有的 `ready`、`reason`、`action` 字段旁包含同样的诊断对象。P05 每条诊断恰含 `code`、`path`、`message`、`stage` 和 `reader`。`reader` 含 `core`、`binary`、`binary_sha256`（小写 64 位十六进制；所选文件无法读取时为 `null`）、`requested` 和 `supported`。`requested` 仅含适用的 `graph_version`、完整 `node_type`、`feature` 和精确 `{id,version}` 包 pin；不适用值为 `null`。`supported` 含排序且唯一的 `graph_versions`、`node_type_versions`、完整 `node_types`、`features`；未知集合为空数组。`path` 是 `/version`、节点 `/node_type`、节点 `/package` 或要求特性的节点／连线设置的精确 JSON 指针；全局所选程序失败指向精确 IsOS 进程节点的 `/settings/command/0`，setup 读取器使用 `""`。`stage` 为 `graph`、`package`、`reader` 或 `feature`。稳定代码为 `reader_identity_invalid`、`reader_graph_unsupported`、`reader_type_unsupported`、`reader_feature_unsupported`、`reader_binary_mismatch`、`reader_package_pin_mismatch`。`message` 点名所选程序及修复动作，不包含原始 stderr、密钥、本机路径或猜测的发行版。

例如缺少必需特性的读取器返回 HTTP `422`，`data` 为 `{"diagnostics":[{"code":"reader_feature_unsupported","path":"/cores/eos/layers/interface/groups/gate/nodes/preview/node_type","message":"aether-runtime lacks vision-gate; select a verified compatible build and Apply again","stage":"feature","reader":{"core":"eos","binary":"aether-runtime","binary_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","requested":{"graph_version":"0.1.0","node_type":"eos.interface.camera_gate@v0.1","feature":"vision-gate","package":null},"supported":{"graph_versions":["0.1.0","1.0.0"],"node_type_versions":["0.1","1.0"],"node_types":["eos.interface.camera_gate@v0.1"],"features":["log-dir"]}}}]}`，`error` 为 `"reader preflight failed"`。所选程序不可读取或角色不符时，spawn 前报告实际角色和 `reader_binary_mismatch`。身份格式错误时报告 `reader_identity_invalid`；若响应不可相信，受支持集合均为空。

`isos version check --json` 仅在 IsOS 已针对该图核验一个已安装或已索引发行版的签名／验证清单、精确资产摘要、核心角色、读取集合、必需特性及包贡献时，才可给出 `compatible_upgrade`。该字段为 `null` 或单个精确发行版 `{id,version,sha256}`；semver 范围、Git 分支、猜测 URL 或较新的产品版本均不够。列出候选不安装、不选择、不应用。缺少验证证据时，建议恢复兼容的所选构建或检查归属方发行目录。即使有候选，未来版本和未知版本依然失败。

[`tests/fixtures/reader_features/`](../../../tests/fixtures/reader_features/) 中的 P05 夹具冻结实现和跨仓库重放的正例、负例及不停止行为。同伴 EOS／EGIA 契约审查通过后，本目标才可标为已实现。
