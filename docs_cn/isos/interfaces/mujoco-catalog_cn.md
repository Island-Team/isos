# MuJoCo 场景与设备准入

[English](../../../docs/isos/interfaces/mujoco-catalog.md) · [已验证软件包](package-verification_cn.md) · [部署图](node-graph_cn.md) · [MuJoCo 所有者描述文件](https://github.com/Island-Team/isos-mujoco-sim/blob/master/docs/mujoco/interfaces/package.md)

**P02 IsOS 归属方契约；实现待完成。**`island/mujoco-sim@0.1.1` 拥有 `backend-v2.json` 的增量目录及包贡献的进程类型。IsOS 拥有收据验证、目录准入、图选择、修订检查、节点集合合成及下列诊断。EOS 拥有通用相机、底盘、机械臂、原语绑定、相机闸门类型及运行时适配器。目录项不授予执行器路由，也不能证明仿真器私有套接字已就绪。P03 拥有通用 `/node-options` HTTP 封装；本契约只提供已验证的 MuJoCo 资源与不可用原因。

## 输入与兼容

schema v4 `release.json`、P01 收据和 `backend-v2.json` schema `2.0.0` 的版本相互独立。描述文件保留精确包身份、`entrypoint`、`protocol:"mujoco-v2"`、`scenes`、`devices` 和 `runtime`。IsOS 先取得精确已验证包句柄，不能从图或选项请求读取未经验证的路径。P01 检查发行、收据、负载、侧车摘要，常规可执行与场景文件、引用资源包含关系及符号链接安全。已链接文件变化先产生 P01 的 `package_changed` 或 `package_unsafe_path`。未被图固定的其他已安装包不能阻止该图。

| 描述字段 | JSON 类型／单位 | 用途、校验与兼容 |
| --- | --- | --- |
| `catalog_version` | 字符串／— | 唯一 P02 判别字段。仅完整历史结构可缺省；存在时必须为 `0.1.0` 且目标字段齐全。部分混合报 `mujoco_catalog_incomplete`，其他值报 `mujoco_catalog_unsupported`；不按文件名或包版本猜测。 |
| `scenes[].id` | 唯一非空 slug／— | 图的 `settings.scene` 稳定值；当前为 `pick-place` 与 `navigation`，不存路径。 |
| `scenes[].label`、`.description` | 非空字符串／— | 展示名称与用途；P02 必需。历史描述可以只读地用 ID 展示，不改字节。 |
| `scenes[].profile` | 枚举字符串／— | 只能为 `eos_benchmark_pick_place_v1` 或 `eos_benchmark_navigation_v1`，且与唯一受支持的 MJCF 自定义数值标记及所需模型元素一致。须安全解析 XML 并做引擎加载测试；字符串搜索不足以验证。 |
| `scenes[].path`、`.sha256` | 安全相对路径；64 位小写十六进制 SHA-256／字节 | P01 已验证的常规 MJCF 文件及精确内容。P02 在发现、Validate、Apply、Run、Restart 时复查所选场景摘要；P01 负载摘要另行覆盖引用资源。选项值不能用路径。 |
| `devices` | 三个 slug ID 的对象／— | 本发行版恰为 `camera:cam-0`、`base:base-0`、`manipulator:arm-0`，三者不同；保留旧字符串映射格式。 |
| `device_catalog[]` | 三个对象的数组／— | 每个 `devices` 类别和 ID 恰一项，无额外或重复 ID。每项有非空 `label`、`description` 和非空、唯一、属于已声明场景的 `scene_ids`；设备只在所选场景支持时可用。 |
| `runtime.mujoco_version`、`.environment_variable`、`.library` | 版本、环境变量名、安全相对路径／— | 保留 `3.16.0`、`MUJOCO_ROOT`、`lib/libmujoco.so.3.16.0`。IsOS 在图准入和启动检查库；文件名吻合本身不能证明就绪时的实际加载 ABI。`MUJOCO_ROOT` 是主机运行输入，不是图选择值或密钥。 |

缺少 `catalog_version` 且没有任何目标元数据的描述文件仅供历史图使用：保留先前验证的 `1.0.0` 图行为和 ID，不发布 P02 选择项或 `0.1.0` 预设。没有判别字段却含目标字段，或有判别字段却缺目标字段，均在发布选项／节点集合前失败。新语义不改写旧图字节。精确形状见[跨归属方夹具](../../../tests/fixtures/scene_catalog/README.md)。

## 图与节点集合准入

包贡献位于 `eos/process/mujoco` 的 `eos.process.mujoco@v0.1`，并保留供已验证历史读取器使用的 `eos.process.mujoco@v1.0`。新进程类型有非空类型描述；`scene` 设置声明 `selector:{"provider":"package","resource":"scene","package_from":"node.package"}`。`socket` 为仅所有者可访问的 Unix 路径，`env` 为可选字符串表，`start_timeout_ms` 为正整数毫秒；`status` 和 `health` 输出带完整描述、`in_process` 传输及 `state` 数据类。进程实例固定 `node.package:{"id":"island/mujoco-sim","version":"0.1.1"}`，不保存可执行或场景路径。侧车可声明 `mujoco-v2` 适配器元数据，但不能自称 EOS 已编译它，也不能创建执行器／安全旁路。

已验证的 `sim-hardware` 集合只能合成 `0.1.0` 草稿。七个节点包括包贡献的进程类型，以及 EOS 通用 `@v0.1` 相机、底盘、机械臂、底盘／机械臂原语绑定和相机闸门类型。它要求起始图已有 `eos.dispatcher.control@v0.1`，不重复该节点，也不虚构 `isos.ui.vision@v0.1` 类型。`scene`、`camera_device`、`base_device`、`manipulator_device`、`socket` 参数保存稳定 ID 和私有套接字路径；默认分别为 `pick-place`、`cam-0`、`base-0`、`arm-0`。IsOS 只对包贡献的进程类型注入 `node.package`。通用硬件实例使用精确的 `node.adapter.package` 固定版本、`simulator_node` 引用和选定 `device_id` 设置。合成返回草稿前须验证精确收据、描述形状、所选场景／设备类别及场景适用性；合成不授予启动权限。改变场景须按修订 Apply 并重启子进程，不得原位修改。

Validate、Apply、迁移提交、Run、Restart 在所有者 setup 或启动子进程前，针对精确固定的进程及各通用适配器重复 P01 和 P02 检查。IsOS 检查所选场景的 profile、摘要及引用资源，按 `devices` 与 `device_catalog` 检查设备 ID，验证主机 MuJoCo 库可用性、图修订及 EOS 适配器可用性。Apply 失败不改变保存字节和活动修订；Run／Restart 失败不替换活动子进程。按需发布新已验证发行版或新图修订。当前 IsOS 合成器仍只接受 `1.0.0`，当前 `BackendDescriptor` 也拒绝目标字段；实现须一起改变。P03 随后按 `(node_path,setting)` 展示可用选项，不改变本准入边界。

## 稳定诊断

P02 沿用 P01 的有界 `{code,path,message,id,version,stage}` 诊断和 HTTP 422 图准入封装。直接 install／link／verify 使用 `path:""`；夹具的 `mutation.path` 指向被测试的描述字段。图路径使用候选图的实际实例 ID 和 JSON Pointer 转义。目录元数据使用 `stage:"descriptor"`，选定值或主机库使用 `stage:"admission"`。消息提供有限修复提示，不包含原始 XML、绝对本地路径或环境值。

| 失败 | `code` | 图中的 `path` | `stage` |
| --- | --- | --- | --- |
| 目标判别字段或字段缺失／部分存在 | `mujoco_catalog_incomplete` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/package` | `descriptor` |
| 未知目录版本 | `mujoco_catalog_unsupported` | 同上 | `descriptor` |
| 无效场景 ID、描述、profile 字面值或声明 | `mujoco_scene_invalid` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/settings/scene` | `descriptor` |
| 场景 profile／标记／模型不符 | `mujoco_scene_profile_mismatch` | 同上 | `admission` |
| 场景摘要不符 | `mujoco_scene_digest_mismatch` | 同上 | `admission` |
| 所选场景未知 | `mujoco_scene_unavailable` | 同上 | `admission` |
| 设备映射／目录无效 | `mujoco_device_invalid` | 对应通用节点的 `/adapter/settings/device_id` | `descriptor` |
| 所选设备未知、类别错误或场景不支持 | `mujoco_device_unavailable` | 对应通用节点的 `/adapter/settings/device_id` | `admission` |
| MuJoCo 库缺失或不兼容 | `mujoco_library_unavailable` | `/cores/eos/layers/process/groups/mujoco/nodes/simulator/package` | `admission` |

对已封存包，P01 `package_changed`（`payload` 阶段）及 `package_unsafe_path`（所属阶段）优先。测试描述语义错误时，须在发布收据前构造候选包；修改已封存描述文件只能得到 P01 篡改错误。[负例夹具](../../../tests/fixtures/scene_catalog/negative-cases.json)固定直接和图阶段的 `{code,path,stage}`，包括底盘／机械臂路径和过期修订的 HTTP 409。P03 可将这些有限原因展示为不可用选项，但不改变错误身份。
