# 节点设置选项

[English](../../../docs/isos/interfaces/node-options.md) · [本地 API](local-api_cn.md) · [MuJoCo 目录](mujoco-catalog_cn.md) · [已验证软件包](package-verification_cn.md)

**P03 IsOS 归属方契约。** IsOS 为草稿或精确保存的图解析展示选项。调用方是 IsosUI 或其他经授权的本机 API 客户端；传输为 `GET`／`POST /api/isos/v1/node-options`，沿用 schema-1 `{schema_version,ok,data,error}` 封装与 Host／Origin 检查。选项只供展示，不是软件包收据、图准入、启动能力或执行器命令授权。Validate、按修订 Apply、Run 和 Restart 均重新检查当前图与精确软件包固定版本。EOS 拥有通用硬件类型及已编译适配器能力；MuJoCo 包只贡献进程类型和已验证资源目录；IsosUI 拥有编辑控件。

## 输入与输出

`GET` 使用已保存图的原始字节；无图时使用可编辑的 `0.1.0` 初始草稿。`POST` 接收 `{"schema_version":1,"graph":{...}}`，不写入。两者按所提交图的版本和精确注册类型 ID 解析，不规范化或迁移保存字节。有效的 `0.1.0` 草稿可含不可用选项；接口仍返回 HTTP 200，便于编辑器修复。未知图版本、未知类型或无效选择器声明返回 HTTP 422，`data.diagnostics` 指向 `/version`、受影响的 `/node_type` 或注册设置路径。API JSON／schema 版本错误返回 HTTP 400。安全的 `1.0.0` 图保留原有三个响应集合；其确实注册的旧选择器可只读展示，但 P03 不虚构 `0.1.0` MuJoCo 目录，也不修改旧图字节。

`data` 保留 `adapter_families`、`primitive_bindings`、`targets`，新增 `setting_options`；前三者的现有形状和旧版含义不变。新集合按 **`(node_path, setting)` 成对**分组，每个现存节点上受支持的选择器恰有一组。分组按 UTF-8 `node_path`、再按 `setting` 升序；候选项（含合成的已选值）按稳定 `value` 的 UTF-8 字节序排列。不得有重复分组或重复候选 ID。`node_path` 是不带前导斜杠的 `core/layer/group/node` 四段路径。公开的 `setting` 是 `scene` 等注册键，或明确由 IsOS 定义的嵌套键 `adapter.settings.device_id`、`adapter.settings.simulator_node`。诊断 JSON Pointer 使用 `/cores/<core>/layers/<layer>/groups/<group>/nodes/<node>`，每段均按 RFC 6901 转义。

| 字段 | JSON 类型／单位 | 用途与校验 |
| --- | --- | --- |
| `setting_options` | 数组／— | 图中完整且有序的选择器分组；无选择器时为 `[]`。 |
| `setting_options[].node_path` | 字符串／四段路径 | 精确实例身份；四段均不可空。 |
| `setting_options[].setting` | 字符串／点分设置键 | 精确注册或内建选择器身份；必需。 |
| `setting_options[].selected` | 字符串或 null／稳定 ID | 精确回显当前图值；缺失时为 null，不暗中替换。 |
| `setting_options[].options` | 数组／— | 有序且唯一的选项；非空已保存 ID 不在已验证候选中时添加一项合成不可用行。 |
| `options[].value` | 字符串／稳定 ID | 精确图设置值；非空、组内唯一，包资源 ID 限于描述文件作用域，绝不是路径。 |
| `options[].label` | 字符串／展示文本 | 已验证描述文件的非空名称，或经过转义且限制长度的已保存 ID。 |
| `options[].description` | 字符串／展示文本 | 已验证描述文件的非空说明，或长度受限的修复说明。 |
| `options[].available` | 布尔值／— | 资源在当前草稿上下文及精确固定版本下是否合格；不表示其他图字段均有效。 |
| `options[].reason_code` | 字符串或 null／— | 不可用时稳定的 P01／P02 代码；可用时为 null。 |
| `options[].reason` | 字符串或 null／展示文本 | 不可用时经过清理的修复动作；可用时为 null。不得含本地路径、XML、环境值或收据正文。 |

即使包缺失、变化或只有旧描述文件，仍展示不可用的已选 ID。合成行保持精确 `value`，用经过转义且限长的标签、`description:"Saved value is not in the verified catalog"`、`available:false` 和原因代码。文件名、绝对路径、可执行程序路径、`MUJOCO_ROOT` 值及软件包 sidecar 都不能作为选项 ID。缺失或格式错误的已选值仍须由图校验拒绝；非字符串值不得强制转换成 ID。选项行只可缓存到产生它的精确草稿请求。客户端在发送更新的草稿请求后丢弃旧响应；GET 响应属于 `GET /node-graph` 给出的保存修订。Apply 输入不接收选项响应或客户端缓存。

## 提供者与 MuJoCo 选择器

对注册的 `Setting.selector`，IsOS 只按声明的 `package`、`registry` 或 `graph` 提供者及资源分派。`package` 选择器必须声明 `package_from`（`node.package` 或 `node.adapter.package`），并通过 P01 验证器解析精确 `{id,version}`；不得扫描最新版本或其他已安装版本。未知提供者／资源组合或无效声明在所属设置处拒绝，不猜测选项。未来资源选择器复用此封装。P03 不执行 `egia.tensor`、任意模型权重或包代码。

已验证的 `eos.process.mujoco@v0.1` 进程的 `scene` 设置声明 `{"provider":"package","resource":"scene","package_from":"node.package"}`。目标 `catalog_version:"0.1.0"` 描述文件中的选项取自 `scenes[].id`、`label` 和 `description`；当前精确值为 `navigation`、`pick-place`。旧进程 `@v1.0` 与完整历史描述文件保留只读旧行为；历史描述文件不能提供目标目录或新预设。部分目标描述文件报告 P02 `mujoco_catalog_incomplete`，不能暗中回退。

对于 `adapter.type:"mujoco-v2"` 的 EOS 通用 `eos.hardware.camera@v0.1`、`eos.hardware.base@v0.1`、`eos.hardware.manipulator@v0.1` 节点，IsOS 增加两个内建选择器分组：`adapter.settings.device_id` 与 `adapter.settings.simulator_node`。提供者是已验证的精确 `node.adapter.package`，并须与引用的 MuJoCo 进程 `node.package` 相同。进程引用由 `adapter.settings.backend` 指定，且须指向 `references` 中的节点；该进程的 `settings.scene` 决定场景适用性。候选项来自相应硬件类别的 `device_catalog[]`，使用稳定 `id`、`label`、`description`；若 `scene_ids` 不含当前场景则不可用。两组提供相同的候选 ID；编辑器选择时同时写入两个字段。外部包不能重定义这些 EOS 类型、位置、适配器 ABI 或安全路线。即使两个 ID 都存在，`device_id` 与 `simulator_node` 不等也在通用节点的 `/adapter/settings/device_id` 指针拒绝 Apply。

精确固定版本缺失、未经验证或变化时，其选项不可用；已链接载荷变化优先报告 P01 `package_changed`。缺失的场景或设备以合成不可用行出现。改变场景或固定版本后，须从该草稿重新计算全部依赖设备行。主机 MuJoCo 库缺失时，目录行必须以 `mujoco_library_unavailable` 标为不可用，但不会把包路径变成选项值。EOS 未编译的适配器沿用现有 adapter-family 原因并保持不可用，包声明不能使其通过 Apply。每行可按 P01 收据、P02 描述文件、场景／设备适用性、运行库、EOS 适配器能力顺序说明首个阻塞原因；Apply 仍返回权威图诊断。

## Apply 与可观察错误

`POST /node-options` 不要求保存图修订；`PUT /node-graph` 与 CLI Apply 必须提供。IsOS 在图锁内比较调用方期望的保存字节 SHA-256，然后在发布前重新执行 P01 精确固定版本验证、P02 目录和所选 ID 检查、通用适配器双字段／引用检查、归属方验证及生成配置预检。最终锁内重读软件包。过期选项响应不能授权旧选择；过期图修订返回 HTTP 409，沿用 `error:"node graph revision changed; reload before applying"`。包变化或所选值无效返回 HTTP 422，附 `data.diagnostics:[{code,path,message,id,version,stage}]`。Validate 在常规 `valid:false` 响应中返回同一诊断。拒绝时不改变保存字节、保存修订、活动修订或子进程。Run 与 Restart 在替换子进程前重复软件包及选择检查。

| Apply 条件 | 精确 `code`／`stage` | 精确图 JSON Pointer |
| --- | --- | --- |
| 精确进程固定版本 `island/mujoco-sim@0.1.1` 缺失 | `package_missing_file`／`manifest` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/package` |
| 已链接进程载荷或场景变化 | `package_changed`／`payload` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/package` |
| 部分目标描述文件 | `mujoco_catalog_incomplete`／`descriptor` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/package` |
| 选项视图过期后所选场景未知 | `mujoco_scene_unavailable`／`admission` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/settings/scene` |
| 设备未知、类别错误、场景不适用、双字段不等、与进程不同但独立验证过的适配器固定版本或 backend 引用错误 | `mujoco_device_unavailable`／`admission` | `/cores/eos/layers/hardware/groups/<kind>/nodes/<id>/adapter/settings/device_id` |
| 主机运行库缺失 | `mujoco_library_unavailable`／`admission` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/package` |

适配器指针的 `kind` 与实例 `id` 使用实际值；[P02 契约](mujoco-catalog_cn.md#稳定诊断)还冻结场景／profile／摘要／设备故障，[P01](package-verification_cn.md)拥有收据诊断。若进程与适配器固定版本同时损坏，以 P01 的规范首个固定版本为准；选项接口的结果不能掩盖它。[P03 夹具](../../../tests/fixtures/node_options/README.md)冻结正例行、过期与负例、精确代码／路径／阶段及不写入不变量。
