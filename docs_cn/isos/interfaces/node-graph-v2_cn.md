# 共享 v2 图与 IsOS 进程节点

[English](../../../docs/isos/interfaces/node-graph-v2.md)

位于 `~/.config/isos/deployments/<name>.nodes.json`（或 `ISOS_HOME/config/deployments/`）的 `2.0.0` 图同时配置 IsOS、EOS 与仅供设计的 EGIA 节点。节点类型声明 core、layer、group、字段与端口；节点实例提供 ID 和设置。当前节点类型版本为 `v1.0`。应用前以及运行前都执行 `aether-profile validate --file <graph>`；IsOS 随后校验软件包进程投影。图的 SHA-256 是保存修订号，运行中的进程保持其修订号直到重启。

`isos init` 只在图不存在时创建可编辑图，。新草稿包含 EOS 进程节点与调度器；预览已安装软件包的节点集合后再添加硬件。`isos config validate <file>` 校验软件包和图；`isos config apply <file>` 保存规范化 JSON；`isos run`、`restart`、`stop`、`status` 和 `logs <node>` 通过仅所有者可访问的直接子进程管理器工作。EOS 在准入硬件操作前启动 MuJoCo，并在 EOS 停止时终止它。安装或链接软件包不会改变运行中的部署。

## 后端节点

新的软件包贡献 MuJoCo 后端使用 `eos.process.mujoco@v1.0`，路径为 `cores/eos/layers/process/groups/mujoco/nodes/<id>`。精确包版本写在 `profile.package:{id,version}`；设置包含 `scene`、`socket`、可选 `env` 与 `start_timeout_ms`，类型和限制见下表。旧内置后端类型及其 `settings.package`/`settings.version` 对已保存图继续有效。软件包贡献的相机、底盘、机械臂实例位于 `cores/eos/layers/hardware/groups/mujoco`，设备类别由注册的节点类型确定，不由共同分组推断。

路径：`cores/eos/layers/process/groups/backend/nodes/<id>`。类型：`eos.process.backend@v1.0`。节点只配置运行方式；`backend-v2.json` 是软件包元数据，相机帧和运动命令使用独立的 Rust 数据结构。可执行文件和场景路径由 IsOS 从校验过的软件包推导，不在节点中填写命令。

| 设置 | Rust 类型 | 单位 | 用途与校验 |
| --- | --- | --- | --- |
| `package` | `String` | 软件包 ID | 必填精确 ID，首个为 `island/mujoco-sim`。 |
| `version` | `String` | 软件包版本 | 必填已安装或已链接的精确版本，首个为 `2.0.0`。 |
| `scene` | `String` | 场景 ID | 必填元数据声明的 ID，选择 XML 场景。 |
| `socket` | `PathBuf` | Unix 路径 | 必填绝对路径，位于 IsOS 运行目录下、仅所有者可访问，最多 107 字节。EOS 适配器必须使用相同路径。 |
| `env` | `BTreeMap<String,String>` | 环境变量 | 可选，仅传给仿真器子进程。`MUJOCO_ROOT` 指向库；`MUJOCO_GUI=1` 在有图形环境的主机上请求可见窗口。键仅允许 ASCII 字母、数字或 `_`，值不得含 NUL。 |
| `start_timeout_ms` | `u64` | 毫秒 | 就绪期限，默认 30000，范围 1–120000。 |

应用和运行时校验后端描述文件的 `entrypoint`、`scenes`、`devices`、`protocol` 与 MuJoCo 运行环境。IsOS 检查场景引用的资源，并把其内容纳入链接指纹。每个 EOS 硬件适配器都要在 `references` 中列出后端，令 `adapter.settings.backend` 等于其规范路径、`socket_path` 等于后端套接字，且 `device_id`/`simulator_node` 等于软件包为相机、底盘或机械臂声明的设备 ID。
IsOS 将已验证的可执行文件、场景路径和 MuJoCo 库路径传给 EOS 运行时。EOS 启动仿真器，把后端 `env` 值限定在该子进程，并等待配置的 Unix 套接字就绪。

## 组件进程节点

路径：`cores/isos/layers/process/groups/component/nodes/<id>`。类型：`isos.process.component@v1.0`。

| 设置 | Rust 类型 | 单位 | 用途与校验 |
| --- | --- | --- | --- |
| `command` | `Vec<String>` | argv | 必填绝对可执行文件及最多 63 个有界参数；不经 shell。IsOS 检查文件存在且可执行。 |
| `cwd` | `Option<PathBuf>` | 路径 | 可选绝对工作目录。 |
| `env` | `BTreeMap<String,String>` | 环境变量 | 可选；键仅允许 ASCII 字母、数字或 `_`，值不得含 NUL。 |
| `ready_tcp` | `Option<String>` | host:port | TCP 就绪探针，与 `ready_unix` 互斥。 |
| `ready_unix` | `Option<PathBuf>` | Unix 路径 | 套接字就绪探针，绝对路径、最多 107 字节，与 `ready_tcp` 互斥。 |
| `start_timeout_ms` | `u64` | 毫秒 | 就绪期限，默认 30000，范围 1–120000。 |
| `restart` | `Restart` | 策略 | `never`、`on_failure` 或 `always`。 |
| `max_restarts` | `u32` | 次数 | 自动重启上限，最多 10。 |
| `depends_on` | `Vec<String>` | 规范节点路径 | 前置进程，须存在、唯一且无环。 |

管理器按依赖顺序启动直接子进程，等待就绪；停止时逆序向进程组发送 TERM，两秒后发送 KILL。提供者故障时先停止依赖者再恢复。分类轮转日志位于 `$XDG_DATA_HOME/isos/logs/<deployment>/<source>.log`；旧原始进程日志仍位于 `$XDG_DATA_HOME/isos/process-logs/<deployment>/<node>.log`。状态包含已保存/运行中修订、固定软件包、PID、重启次数和最近管理错误。私有 Unix 管理器 API 保留 schema 1 封装，使用 `v2_start`、`v2_restart`、`v2_stop`、`v2_status` 操作。

## 可选的 EOS 控制节点

编写的图可以在 `cores/eos/layers/control` 中声明 `manual_0`、`base_velocity_0`、`arm_joint_0` 和 `demo_0`。对应类型为 `eos.control.manual@v1.0`、`eos.control.primitive@v1.0`、`eos.control.skill@v1.0`。它们配置命令来源；运行中的请求与结果仍使用独立的有类型数据。每个控制节点都引用 EOS 调度器的规范路径。手动和原语节点还引用目标硬件节点；技能节点引用组成它的原语。这些引用不构成直接执行器路径；现有调度器输出仍是硬件唯一的命令输入。

| 设置 | Rust 类型 | 单位 | 用途与校验 |
| --- | --- | --- | --- |
| 手动 `target` | `String` | 规范节点路径 | 必填底盘或机械臂目标；须为已准入硬件节点并出现在 `references` 中。 |
| 原语 `target` | `String` | 规范节点路径 | 必填硬件目标；须为已准入底盘或机械臂节点并出现在 `references` 中。 |
| 原语 `movement_type` | `String` 枚举 | 运动模式 | 底盘须为 `body_velocity`，机械臂须为 `joint_positions`；目标必须支持该模式。 |
| 技能 `steps` | `Vec<String>` | 有序规范节点路径 | 必填有序原语引用；每个路径须出现在 `references` 中，且解析为已注册原语节点。 |

手动和原语节点必须从调度器接收 `status: Status` 输入，并从目标硬件接收 `base_observation: BaseObservation` 或 `manipulator_observation: ManipulatorObservation` 输入；技能节点必须从调度器接收 `status: Status`。手动和原语节点提供 `request: ControlRequest` 与 `outcome: CommandOutcome` 输出端口；技能节点提供 `outcome: SkillOutcome`。节点 ID 默认由最终类型名和序号组成，例如 `manual_0` 或 `demo_0`；它是图身份，不是运动数据结构。

另见[后端软件包设计](../design/backend-v2_cn.md)和[本地 API](local-api_cn.md)。
