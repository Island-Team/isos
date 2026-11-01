# 共享节点图与 IsOS 进程节点

[English](../../../docs/isos/interfaces/node-graph.md)

按 `core → layer → group → node` 从[节点目录](../layers/README_cn.md)查找 IsOS 自有类型。本页仍负责序列化字段、校验和兼容规则。

**实现状态：**上文 `1.0.0` 图与 `@v1.*` 类型为当前行为。IsOS 首个兼容读取切片已保留原始字节并拒绝不支持的版本；`0.1.0` 写入器、`@v0.1` 目录及显式迁移仍为发行目标。产品、图、节点类型、HTTP 路径和仿真器协议各自版本化。

位于 `~/.config/isos/deployments/<name>.nodes.json`（或 `ISOS_HOME/config/deployments/`）的 `1.0.0` 图同时配置 IsOS、EOS 与仅供设计的 EGIA 节点。节点类型声明 core、layer、group、字段与端口；节点实例提供 ID 和设置。节点类型独立版本化；IsOS 托管进程类型为 `v1.1`，当前 EOS 硬件类型为 `v1.0`。应用前以及运行前都执行 `aether-profile validate --file <graph>`；IsOS 随后校验软件包进程投影。图的 SHA-256 是保存修订号，运行中的进程保持其修订号直到重启。

`isos init` 只在图不存在时创建可编辑图，然后退出。新草稿包含 EOS 进程节点与调度器；预览已安装软件包的节点集合后再添加硬件。`isos config validate <file>` 校验软件包和图；`isos config apply <file>` 保存规范化 JSON。`isos serve` 运行仅所有者可访问的管理器和界面。`isos run` 与 `restart` 保持前台运行；`isos deployment stop` 只停止该图，`isos stop` 关闭管理器及所有所属子进程。EOS 在准入硬件操作前启动 MuJoCo，并在 EOS 停止时终止它。安装或链接软件包不会改变运行中的部署。

## 后端节点

新的软件包贡献 MuJoCo 进程节点使用 `eos.process.mujoco@v1.0`，路径为 `eos/process/mujoco/<id>`。精确包版本写在 `profile.package:{id,version}`；设置包含 `scene`、`socket`、可选 `env` 与 `start_timeout_ms`，类型和限制见下表。相机、底盘、机械臂设备分别使用 `eos/hardware/camera/<id>`、`eos/hardware/base/<id>` 和 `eos/hardware/manipulator/<id>` 下的通用 EOS 类型。每台设备在 `profile.adapter` 中选择固定软件包版本的适配器系列；通用硬件节点类型不归软件包所有。

| 设置 | Rust 类型 | 单位 | 用途与校验 |
| --- | --- | --- | --- |
| `scene` | `String` | 场景 ID | 必填元数据声明的 ID，选择 XML 场景。 |
| `socket` | `PathBuf` | Unix 路径 | 必填绝对路径，位于 IsOS 运行目录下、仅所有者可访问，最多 107 字节。EOS 适配器必须使用相同路径。 |
| `env` | `BTreeMap<String,String>` | 环境变量 | 可选，仅传给仿真器子进程。`MUJOCO_ROOT` 指向库；`MUJOCO_GUI=1` 在有图形环境的主机上请求可见窗口。键仅允许 ASCII 字母、数字或 `_`，值不得含 NUL。 |
| `start_timeout_ms` | `u64` | 毫秒 | 就绪期限，默认 30000，范围 1–120000。 |

应用和运行时校验后端描述文件的 `entrypoint`、`scenes`、`devices`、`protocol` 与 MuJoCo 运行环境。IsOS 检查场景引用的资源，并把其内容纳入链接指纹。每个 EOS 硬件适配器都要在 `references` 中列出后端，令 `adapter.settings.backend` 等于其规范路径、`socket_path` 等于后端套接字，且 `device_id`/`simulator_node` 等于软件包为相机、底盘或机械臂声明的设备 ID。
IsOS 将已验证的可执行文件、场景路径和 MuJoCo 库路径传给 EOS 运行时。EOS 启动仿真器，把后端 `env` 值限定在该子进程，并等待配置的 Unix 套接字就绪。

## 组件进程节点

路径：`cores/isos/layers/process/groups/component/nodes/<id>`。当前类型：`isos.process.component@v1.1`；已保存的 `@v1.0` 节点会收到转换提示，Apply 不会自动迁移。

| 设置 | Rust 类型 | 单位 | 用途与校验 |
| --- | --- | --- | --- |
| `role` | `String` | 核心 ID | 托管的 `isos.process.component@v1.1` 节点必填 `eos` 或 `egia`；选择已配置的核心程序目录。 |
| `args` | `Vec<String>` | argv 参数 | 传给 `role` 确定的程序；最多 63 个有界字符串，不经 shell，也不包含程序路径。 |
| `command` | `Vec<String>` | argv | 已废弃，Apply 会拒绝；请使用 `role` 和 `args`。 |
| `cwd` | `Option<PathBuf>` | 路径 | 可选绝对工作目录。 |
| `env` | `BTreeMap<String,String>` | 环境变量 | 可选；键仅允许 ASCII 字母、数字或 `_`，值不得含 NUL。 |
| `ready_tcp` | `Option<String>` | host:port | TCP 就绪探针，与 `ready_unix` 互斥。 |
| `ready_unix` | `Option<PathBuf>` | Unix 路径 | 套接字就绪探针，绝对路径、最多 107 字节，与 `ready_tcp` 互斥。 |
| `start_timeout_ms` | `u64` | 毫秒 | 就绪期限，默认 30000，范围 1–120000。 |
| `restart` | `Restart` | 策略 | `never`、`on_failure` 或 `always`。 |
| `max_restarts` | `u32` | 次数 | 自动重启上限，最多 10。 |
| `depends_on` | `Vec<String>` | 规范节点路径 | 前置进程，须存在、唯一且无环。 |

管理器按依赖顺序启动直接子进程，等待就绪；停止时逆序向进程组发送 TERM，两秒后发送 KILL。提供者故障时先停止依赖者再恢复。IsOS 自身诊断及选定的结构化子进程事件写入轮转的 `$XDG_DATA_HOME/isos/logs/<deployment>/<source>.log` 文件；普通子进程输出不再镜像。状态包含已保存/运行中修订、固定软件包、PID、重启次数和最近管理错误。私有 Unix 管理器 API 保留 schema 1 封装，使用 `deployment_start`、`deployment_restart`、`deployment_stop`、`deployment_status` 操作。

## 当前 EOS 控制节点与旧技能兼容

现有 `1.0.0` 图可能在 `cores/eos/layers/control` 包含 `manual_0`、`base_velocity_0`、`arm_joint_0` 和 `demo_0`。历史类型是 `eos.control.manual@v1.0`、`eos.control.primitive@v1.0`、`eos.control.skill@v1.0`。`demo_0` 与 `eos.control.skill@v1.0` **仅用于兼容**：新 `0.1.0` 图不编写图内技能序列或 workflow 节点。它们配置命令来源；运行中的请求与结果仍使用独立的有类型数据。每个控制节点都引用 EOS 调度器的规范路径。手动和原语节点还引用目标硬件节点；技能节点引用组成它的原语。这些引用不构成直接执行器路径；现有调度器输出仍是硬件唯一的命令输入。

| 设置 | Rust 类型 | 单位 | 用途与校验 |
| --- | --- | --- | --- |
| 手动 `target` | `String` | 规范节点路径 | 必填底盘或机械臂目标；须为已准入硬件节点并出现在 `references` 中。 |
| 原语 `target` | `String` | 规范节点路径 | 必填硬件目标；须为已准入底盘或机械臂节点并出现在 `references` 中。 |
| 原语 `movement_type` | `String` 枚举 | 运动模式 | 底盘须为 `body_velocity`，机械臂须为 `joint_positions`；目标必须支持该模式。 |
| 旧技能 `steps`（仅兼容） | `Vec<String>` | 有序规范节点路径 | 必填有序原语引用；每个路径须出现在 `references` 中，且解析为已注册原语节点。 |

手动和原语节点必须从调度器接收 `status: Status` 输入，并从目标硬件接收 `base_observation: BaseObservation` 或 `manipulator_observation: ManipulatorObservation` 输入；技能节点必须从调度器接收 `status: Status`。手动和原语节点提供 `request: ControlRequest` 与 `outcome: CommandOutcome` 输出端口；技能节点提供 `outcome: SkillOutcome`。节点 ID 默认由最终类型名和序号组成；`manual_0` 是当前例子，`demo_0` 仅为旧图兼容例子。节点 ID 是图身份，不是运动数据结构。

另见[后端软件包设计](../design/backend_cn.md)和[本地 API](local-api_cn.md)。

## 标准节点路径与原语绑定


`eos.dispatcher.primitive_base@v1.0` 与 `eos.dispatcher.primitive_manipulator@v1.0` 是绑定节点。其 `settings` 用字符串 `primitive_name`、`primitive_version`、`movement_type` 指定原语。硬件 `inputs.command` 是来自绑定节点的 `{node,output}` 边数组；每个绑定输出必须连接到一个兼容硬件节点。适配器实现由硬件实例的 `adapter.type` 选择，与通用硬件节点类型独立。IsOS Vision 配置为 `isos.ui.vision@v1.0`，放在 `isos/ui/panel` 下，是内部 UI 节点，不作为进程监管。

## 发行目标：图 `0.1.0` 与兼容读取——尚未实现

**状态：**上文 `1.0.0` 图和 `@v1.*` 类型已实现；本节为目标，不可按当前命令使用。产品、图、节点类型、HTTP 路由和仿真器协议独立版本化。新图写 `version:"0.1.0"`，新类型用 `@v0.1`。读取器继续接受已有安全 `1.0.0` 图及已注册 `@v1.0`／`@v1.1` 类型，不批量替换类型 ID。未注册类型按路径给出迁移诊断；旧任意命令 `isos.process.component@v1.0` 绝不执行。新 `isos.process.component@v0.1` 按上述 `role` 与有界 `args` 选择程序。既有 HTTP 路径、protobuf 包名、发行清单 schema 4 和 MuJoCo socket 协议 `2.0.0` 不改。

| 图字段 | JSON 类型／单位 | 所有者与用途 | 默认值和校验 | 兼容扩展 |
| --- | --- | --- | --- | --- |
| `version` | 字符串／图格式 | IsOS 选择解析器与写入器 | 新草稿必填 `0.1.0`；读取器也接受 `1.0.0` | 不暗中改写旧字节 |
| `robot.id`、`robot.name` | 字符串／— | 操作员身份与显示名 | 必需非空 | 仅增加元数据 |
| `cores` | 核心 ID 到对象／— | IsOS 图根，各核心拥有子树 | 必需已注册键；未知核心拒绝 | 新核心需注册所有者 |
| `layers`、`groups`、`nodes` | 分层键对象／— | 核心位置，由类型注册表校验 | 已注册段；唯一四段 `core/layer/group/node` 路径 | 可新增已注册位置 |
| `node.id` | 字符串／路径段 | 实例身份，等于 `nodes` 键 | 必需合法路径段 | 不暗中改名 |
| `node.node_type` | 字符串／版本化 ID | 注册表选择行为 | 必需已注册且位置相符 | 旧 ID 保持原义 |
| `node.label`、`node.description` | 字符串／— | 操作员可读实例标题与功能 | 必需；旧实例可有空说明，新实例说明不可空 | 保留原说明 |
| `node.package`、`node.adapter.package` | `{id,version}`／精确版本 | IsOS 解析贡献类型或适配器 | 包贡献类型／适配器必需；ID 为 `namespace/name`，版本为数字 `x.y.z` | 改版本需 Apply |
| `node.settings` | 对象／类型专用 | 类型所有者声明设置，IsOS 校验包选择器 | 无设置时空对象；未知设置按类型契约拒绝 | 可选新设置须有默认值 |
| `node.inputs` | 端口名到来源或来源数组／— | 注册表校验有类型连接 | 无输入时空对象；来源解析到注册 `data_type` ID 完全相同的输出，且基数匹配；schema `x-unit` 注解不允许隐式换算 | 可选边元数据见下表 |
| `node.references` | 规范路径字符串数组／— | 所有者声明非数据依赖 | 默认空；按类型要求存在且唯一 | 新引用不得增加执行器旁路 |
| `node.adapter`、`capabilities`、`health_timing`、`observation`、`movement`、`frame_id`、`joint_names` | 类型专用对象／数组／字符串 | EOS 硬件配置 | 由 [EOS 硬件接口](https://github.com/Island-Team/Island-eos)（`docs/eos/interfaces/hardware.md`）定义；IsOS 不臆造硬件含义 | EOS 定义兼容默认值 |

注册**类型**另有非空 `description`，说明功能、输入、输出和操作员可见效果。旧已验证侧车缺此字段时，仅在目录中生成回退说明，不修改侧车字节。新设置可选 `selector` 由 [EOS 节点贡献契约](https://github.com/Island-Team/Island-eos)（`docs/eos/interfaces/node-contributions.md`）定义；IsOS 拥有[解析选项响应](local-api_cn.md#发行目标设置选项尚未实现)。

| `node.inputs.<port>` 来源字段 | JSON 类型／单位 | 用途 | 默认值与校验 | 兼容 |
| --- | --- | --- | --- | --- |
| `node` | 字符串／规范节点路径 | 输出所有者 | 必需且存在 | 不变 |
| `output` | 字符串／端口名 | 输出端口 | 必需且有类型兼容 | 不变 |
| `priority` | 整数／Linux nice 标度 | 加权投递顺序 | 默认 0；`-20..19`，数值越低优先级越高 | 旧边缺省归一为 0 |
| `frequency_hz` | 数字／Hz | 此边最高投递频率 | 缺省无边限速；存在时须有限且 `>0` | 旧边无上限 |

兼容输入端口的目标 JSON **来源对象片段**（当前程序尚不接受新增边字段）：

```json
{"node":"eos/dispatcher/base/velocity","output":"command","priority":5,"frequency_hz":15.0}
```

声明来源绑定节点后，将此对象放入兼容目的节点的 `inputs.command` 数组；[当前起始模板](../../../templates/generic-graph.toml)给出不带目标边字段的完整图。端口 `expected_rate_hz` 是生产者提示，独立于边限速。EOS 对运行时边使用单调时钟 token bucket 和有界队列的加权公平投递。观测溢出增加可见计数并限速告警；不能准入的命令须明确拒绝或报告不确定结果，绝不可暗中丢弃。停止路由时丢弃非命令排队工作，并通过调度器停止执行器。优先级不改变命令授权，执行器数据仍通过调度器。IsOS 存储和校验边元数据，不调度硬件；IsosUI 显示两个值并标明仅供设计的连线。

读取算法：最多读取 1 MiB，并在 JSON 解码或规范化之前根据**原始已保存字节**计算 SHA-256 修订号。只按精确图版本 `1.0.0` 和 `0.1.0` 分派；对原始节点类型 ID 校验精确注册身份和既有安全行为。本读取器保留原始 JSON 值；既有归属方校验器在类型化投影时应用其已确立的可选字段默认值。共享的规范化投影留待迁移切片实现，不得改动已保存字节或类型身份。未知版本可作为原始 JSON 在工作台查看，但不能校验、应用、生成配置、运行或重启；这些操作在 `/version` 返回支持版本集合。无效 JSON 不能作为图展示。见[架构算法](../design/system_cn.md)。

已实现的首个兼容读取切片保留现有 `1.0.0` 写入器和初始草稿。若尚无已保存图，或已编辑的初始图尚无已应用的 `core-setup/launch-inputs.json` 快照，首次安全 `1.0.0` Apply 使用现有规范 JSON 序列化，并返回实际写入字节的 SHA-256。已应用的 `1.0.0` 图在迁移功能完成前标记为只读；未应用的初始图仍可编辑。对于安全且已保存的图，带匹配修订号的 PUT 或 CLI Apply 若提交结构相同的图，就返回原修订号而不写文件；首次 Apply 之后若图有改动，则返回 `migration_required` 诊断，要求显式迁移。这样避免 JSON 重新序列化暗中改变空白、键顺序和修订号。后续独立迁移操作先预览确定性的旧新差异，要求原始已保存修订号，并且只在校验及核心 setup 成功后原子写入 `0.1.0` 字节。迁移保留 ID、路径、引用、边、精确软件包版本及旧 schema 已接受的字段；未映射的未知字段须拒绝迁移，不能丢弃。不猜测旧类型到新类型的映射。未映射类型可供查看和修复，但不会获得新的执行路径。任意命令 `isos.process.component@v1.0` 及其 `settings.command` 不能校验、应用或启动，也不会自动变成 `role` 与 `args`。旧 `eos.process.backend@v1.0` 可查看，但 Apply 前须有精确且已验证的替代类型。解析、软件包检查、setup 生成或修订号比较失败时，已保存字节及运行中修订号不变；迁移成功也只有在 Restart 后才改变运行中修订号。

固定兼容输入位于 [`tests/fixtures/graph_compat/`](../../../tests/fixtures/graph_compat/README.md)，包含安全的 `1.0.0` 图、空白不同但语义相同的图、未来 `2.0.0` 图、未知类型 ID 和不安全命令进程。这些字节及预期路径属于测试样例，不是新部署模板。

## G03 显式图迁移——目标设计，尚未实现

IsOS 只迁移**已保存**且安全的 `1.0.0` 图。预览与提交读取该文件，不接受编辑器另行提交的替代图。预览最多读取 1 MiB，对原始字节计算哈希，校验精确版本解析器、类型 ID、软件包版本、设置、引用和边，再构造确定的 `0.1.0` JSON 值。仅更改 `/version` 和下表列出的精确类型 ID。实例 ID、路径、描述、设置、引用、边、adapter 元数据及精确 `{id,version}` pin 在 JSON 值上保持不变。可选默认值仅用于校验投影，不写入迁移结果。通用图字段、注册节点类型字段、设置 schema 与软件包贡献 schema 构成接受边界；只有 schema 明确声明的扩展 map 可保留其成员。其他未知字段在其 JSON Pointer 返回 `unmapped_graph_field`，不能经类型化反序列化静默丢弃。规范字节采用 `serde_json::Value` 的字典序 object key、两空格缩进、UTF-8、无尾随换行的序列化，与 `serde_json::to_vec_pretty` 默认行为一致。建议修订号是该规范字节的 SHA-256；预览返回其 JSON 值和有界变更列表；`changes` 超过 256 项时在 `/changes` 返回 `migration_change_limit` 并拒绝，不截断。预览不修改保存字节或生成配置。

| 精确旧类型 | 精确目标类型 | 归属方与前提 |
| --- | --- | --- |
| `eos.hardware.camera@v1.0` | `eos.hardware.camera@v0.1` | EOS 通用 camera 类型已注册，重新校验设置和端口。 |
| `eos.hardware.base@v1.0` | `eos.hardware.base@v0.1` | EOS 通用 base 类型已注册，重新校验设置和端口。 |
| `eos.hardware.manipulator@v1.0` | `eos.hardware.manipulator@v0.1` | EOS 通用 manipulator 类型已注册，重新校验设置和端口。 |
| `eos.dispatcher.control@v1.0` | `eos.dispatcher.control@v0.1` | EOS dispatcher 类型已注册；命令准入仍由 Dispatcher 管理。 |
| `eos.dispatcher.primitive_base@v1.0` | `eos.dispatcher.primitive_base@v0.1` | EOS binding 类型已注册；重新校验目标与命令路由。 |
| `eos.dispatcher.primitive_manipulator@v1.0` | `eos.dispatcher.primitive_manipulator@v0.1` | EOS binding 类型已注册；重新校验目标与命令路由。 |
| `eos.interface.camera_gate@v1.0` | `eos.interface.camera_gate@v0.1` | EOS camera gate 类型已注册；重新校验证据边。 |
| `isos.process.component@v1.1` | `isos.process.component@v0.1` | IsOS 按角色选择的进程类型已注册；重新校验 `role` 和有界 `args`。此类型归 IsOS；EOS 须使用按归属方分区的校验或显式 IsOS contribution 接受，不能把它注册为 EOS 内置类型。 |
| `eos.process.mujoco@v1.0` | `eos.process.mujoco@v0.1` | 仅当**同一精确版本的已验证软件包**贡献新进程类型，且现有 scene/socket 设置通过校验。此前旧节点保持只读并返回 `unmapped_node_type`。 |

只有四个 EGIA 历史设计类型可在 `0.1.0` 建议图中保留原 ID，因为 EGIA G05 reader 明确接受该 envelope 中的它们：`egia.common.source@v1.0`、`egia.common.context@v1.0`、`egia.common.planner@v1.0` 和 `egia.common.guard@v1.0`。它们不会获得新的运行时路由；缺少 EGIA reader 支持时仍在 `/node_type` 阻塞迁移。旧 EOS `eos.control.manual@v1.0`、`eos.control.primitive@v1.0` 和 `eos.control.skill@v1.0` 没有 `@v0.1` 替代类型，不能通过 EOS 的 `0.1.0` 类型版本校验。它们保持只读，在精确节点路径返回 `unmapped_node_type`，提示删除或编写现有的新 binding。`isos.ui.vision@v1.0` 与 `isosui.system.vision@v1.0` 同样保守处理，直到归属方明确注册 `0.1.0` envelope 兼容。`isos.process.component@v1.0` 和任意 `settings.command` 返回 `unsafe_process_node`；`eos.process.backend@v1.0`、软件包专用 `eos.hardware.mujoco_*@v1.0` 与未知 ID **不自动映射**。后者需要操作员编写替代节点并选择精确验证过的软件包/adapter；只改版本后缀不足以完成转换。不会生成图定义的技能序列。

映射完成但归属方 reader 尚未就绪时，预览可返回 `can_apply:false` 和精确诊断。提交必须同时带有已保存原始字节的修订号及预览的建议修订号。发布前，IsOS 要求自有 `0.1.0` registry 和每个启用 core 的 `setup identity` 声明图 `0.1.0`；还须检查**完整精确**类型注册，而不只检查 `supported_node_type_versions`，并对建议图执行各归属方的 `validate` 和 `render`。当前 EOS profile 仍报告标量图 `1.0.0`，当前初始图的 IsOS `component@v0.1` 也没有 EOS 可见的 contribution。在 EOS reader-features 与 IsOS 注册切片完成前，这两点阻止发布。EGIA 的兼容图 envelope 只支持四个旧 common ID，因此新的 `@v0.1` EGIA 阶段仍受阻。产品版本或图 envelope 本身不授权类型准入。

提交在图锁内比较传入的原修订号与当前保存字节 SHA-256，并重新计算建议结果；来源或建议修订过期时不写文件并返回冲突。发布前检查 receipt、链接包指纹、scene/asset、边准入及两个 core 的 render。先 fsync 并发布以**建议修订号**命名的不可变生成配置快照，再 fsync 同目录图临时文件，最后原子 rename 图。图 rename 前失败则保存字节和运行中修订号不变；清理临时文件，未使用的已发布快照可回收。图 rename 后不再执行校验、setup 或启动操作，但仍需 fsync 图的父目录。若此 fsync 失败或连接在 rename 后中断，返回 `commit_outcome_uncertain` 或断线；此时不能断言旧字节仍在，调用方重新读取保存修订号判断提交是否成功。成功迁移也只改变**已保存**修订号；运行中子进程保持 `active_revision`，直到显式 Restart 重新校验精确新图及软件包。

### G03 写入器与快照选择——目标设计

归属方 reader 通过验收后，**缺失**的部署图由 GET 返回可编辑的 `0.1.0` 草稿；`isos init` 和 `isos config init` 只在文件不存在时创建 `0.1.0` 初始图。它们绝不重写已有 `1.0.0` 图。新初始图使用 `eos.dispatcher.control@v0.1` 与 IsOS 自有的 `isos.process.component@v0.1` 按角色选择的子进程。缺失图首次 PUT 必须带 `revision:""`；后续修改 `0.1.0` 的 PUT 必须带精确保存修订号。CLI 修改 `0.1.0` 时要求 `--revision <saved-sha256>`（缺失图使用 `--revision ""`）。两者只在同样的预检和 CAS 成功后写入规范 `0.1.0` 字节。已保存且安全的 `1.0.0` 图仍可查看；结构相同的 Apply 保持无写入，未应用旧初始图可完成旧版首次 Apply，已应用旧图的改动必须迁移。归属方 reader 不可用时，新草稿不能暗中降级为 `1.0.0`：Init 仍可离线创建 `0.1.0` 草稿，但 Apply 返回 `owner_reader_unavailable` 并保持已保存字节不变。

对于 `0.1.0`，生成文件位于 `data/generated/<deployment>/<saved-revision>/`，包含 `core-setup/launch-inputs.json`、`eos/graph.json` 和已启用 core 的覆盖文件。清单记录来源修订、精确图／软件包／程序摘要，以及生成文件路径和摘要；Run 或 Restart 前重新校验。IsOS **仅根据当前已保存图的 SHA-256** 选择此目录并校验清单；不存在可变的 `current` 指针或 symlink。EOS runtime 与 config-server 的启动 `--profile` 参数替换为该目录下经验证的 `eos/graph.json`；已保存图的 `settings.args` 保持不变。旧 `data/generated/<deployment>/core-setup/launch-inputs.json` 布局只为 `1.0.0` Run/Restart 和 GET 已应用状态保留只读兼容；`0.1.0` reader 不回退到它。`GET /node-graph` 按版本、准入和旧版应用状态决定 `read_only`，不依据无关快照目录。`GET /process-status` 从 manager 的已启动修订取得 `active_revision`，不从生成路径推断。

Run 和 Restart 会将已保存的生成字节与从精确已保存图重新执行的归属方 render 比较，因此同时修改文件和清单不能授权不同投影。重新构建核心程序会使启动输入摘要失效。`isos init` 报告过期输入且不修改不可变快照；恢复固定的程序，或创建新的图修订并 Apply 后才能启动。

在归属方投影前，IsOS 会在 `/cores/<core>` 以 `unsupported_core` 拒绝 `eos`、`isos`、`egia` 之外的任何 `0.1.0` core 键。每个节点须匹配精确批准的目标类型身份及其注册的 `core/layer/group` 位置；错误在节点 `/node_type` 返回 `unsupported_node_type` 或 `invalid_node_placement`。旧版 IsOS／IsosUI Vision 身份与未映射的 EOS 控制身份不能进入新图，即使旧版目录仍列出它们。

`0.1.0` 校验握手按归属方分区。IsOS 解析并校验完整图，包括精确节点路径、引用、软件包 pin 和全部跨 core 连线；其自有 registry 校验 `isos.process.component@v0.1` 和其他 IsOS 类型。IsOS 聚合目录合并这些自有定义、EOS 定义及已验证的软件包 contribution。对于 `0.1.0`，IsOS 把完整图交给 EOS setup Validate/Render，由 EOS 仅投影并校验 `eos` core；EGIA setup 校验自己的子树。IsOS **不会**对 `0.1.0` 使用旧整图 `aether-profile validate --file`，否则会错误要求 EOS 拥有 IsOS 进程类型。旧 `1.0.0` 保留既有整图校验。在 G06 之前，建议 `0.1.0` 图中的任意跨 core 边或引用都在精确路径阻塞迁移、Apply 和 Run，包括从旧图保留的连线。启用的归属方 adapter 必须声明 `0.1.0`、成功 Validate/Render，且已启动的 EOS runtime 能读取生成投影，IsOS 才发布图。
