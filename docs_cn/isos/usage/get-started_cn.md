# 首次使用：IsOS 与 MuJoCo

[English](../../../docs/isos/usage/get-started.md)

## 按实际运行顺序阅读

**当前可用：**新图写入 `0.1.0` 和已注册的 `@v0.1` 类型。已知安全的 `1.0.0` 图仍可读取；应用后编辑须显式迁移。设置选择器、统一事件字段和 EGIA 集成属于[后续里程碑](../maintenance/roadmap_cn.md)。操作时先看状态，再查同一阶段的日志；启动事件记录状态变化，状态才代表当前事实。

1. 安装 IsOS 后运行 `isos`。它会创建缺失的开发配置和图，在 16000 启动 API、16001 启动 IsosUI，并打开浏览器；EOS、EGIA 与 MuJoCo 仍停止。再次运行会复用工作台。`isos init` 只初始化文件并退出。

```sh
isos
```

2. 在 EOS、EGIA 与 IsosUI 各自的仓库构建，在 IsOS 仓库的 `config/isos.toml` 中设置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 与 `paths.ui_dist`。执行 `isos version check`（脚本可加 `--json`）。先修复报告的程序缺失或 setup 不匹配，再启动硬件。Rust EOS gateway 完成前，EGIA 可能仍显示阻塞。`isos serve` 不打开浏览器；`isos open` 会按需启动。`isos serve --ui-live` 仅用于前端开发。

新的 `0.1.0` 草稿在 Graph 中显示保存修订号。先 Validate，再用该修订号 Apply。目标图文件不存在时，首次 API PUT 须显式携带空修订号（`"revision":""`）；`isos init` 已保存起始图后则须使用其实际 SHA-256。CLI Apply 对已保存起始图或后续修改使用 `--revision <saved-sha256>`；只有目标文件不存在时才使用 `--revision ""`。归属方 reader 不可用会阻止 Apply，草稿字节保持不变。

3. 构建 MuJoCo 仿真器，用其仓库的 `tools/build-backend-bundle.py` 制作 `0.1.1` 包，并将 `MUJOCO_ROOT` 指向匹配环境。链接精确已验证的包。在 Graph 或通过 `isos pkg preview-set` 预览 `sim-hardware`。起始图只有 EOS 进程和调度器，没有硬件包；P02 的 `0.1.0` 预设加入 MuJoCo 进程、EOS 通用相机、底盘、机械臂、底盘／机械臂原语绑定及相机闸门，不加入 Vision 节点。选择 `pick-place` 或 `navigation` 以及该场景可用的相机／底盘／机械臂 ID，检查草稿，在 Graph 中 Validate 后 Apply。仅安装或链接不会修改图。P02 IsOS 目录准入已生效。P03 节点选项要求每次修改草稿后请求 `POST /api/isos/v1/node-options`；使用其中可用的场景与设备 ID，同时设置每个通用适配器的 `device_id` 和 `simulator_node`，并只用最新保存修订号 Apply。不可用的已保存 ID 仍可见，但会阻止 Apply。CLI 预览、选择流程及精确负例见[软件包操作](packages_cn.md#p03-选择流程)：

```sh
isos pkg link island/mujoco-sim 0.1.1 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
```

4. Apply 后执行 `isos run`，它保持前台运行。在另一终端先用 `isos status --json` 查看保存／活动修订、进程就绪和最近错误，再用 `isos logs all --level info` 查看启动顺序。当前 IsOS 启动行表示启动尝试，状态才确认就绪。MuJoCo 或可加载适配器的软件包加载故障先读 `isos logs packages --level warn`：`package_id`／`package_version` 标识固定发行版，`run_id` 和 `correlation_id` 关联管理器预检失败；再查看命令错误确定修复方法。EOS 与 MuJoCo 来源解释子进程启动。特定故障用 `isos logs eos --level warn` 或 IsosUI Logs 面板调查。日志位于私有数据根目录的 `logs/<deployment>/`；路径、过滤、轮转和游标见[配置与日志](../interfaces/configuration_cn.md)。目标统一事件会增加单条就绪事件和节点字段；当前大多数事件可能没有。

5. Ctrl+C 停止附着的部署。`isos deployment stop` 仅停止图；`isos stop` 也停止独立服务、UI、API 和管理器。重复停止会成功。停后用 `isos status --json` 确认已停止；若停止异常，再查看警告与错误日志。干净停止无需大量周期性消息。

运行前会重新校验精确的图修订、已配置程序、软件包、场景和资源。EOS 启动并持有 MuJoCo 子进程。安装包、修改程序路径或编辑草稿都不会改变运行中的部署；需显式 Apply 和 Restart。参见[软件包命令](packages_cn.md)、[图字段](../interfaces/node-graph_cn.md)及[独立服务契约](../interfaces/services_cn.md)。

对于已应用的 `0.1.0` 图，Run 会在启动前将按修订保存的生成文件与新的归属方 render 比较。如果配置的 EOS 或 EGIA 程序发生变化，`isos init` 会报告过期的启动输入并保留不可变快照。恢复固定的程序，或创建新的图修订并 Apply，然后再运行。

**P04 生成配置首次使用：**选择已安装的 EOS 和 EGIA 发行版，将 `paths.egia_defaults_dir` 和 `paths.egia_factory_defaults_dir` 分别设为该 EGIA 发行版的组件与工厂默认目录，并验证精确的 MuJoCo 软件包固定版本，然后 Validate 并按保存修订 Apply。IsOS 发布为该精确修订选定的私有生成版本。检查 `data/generated/<deployment>/selection.json` 中保存修订对应的条目、持久的 `v2/required/<revision>.json` 标记及选中目录的 `v2/<revision>/<generation>/manifest.json`；摘要须与保存图和所选已安装输入匹配。P04 Apply 后若选择记录缺失，Run 会阻止启动，不会退回旧 G03 快照。末级目录包含 EOS `graph.json`，启用 EGIA 时还包含模型、提示、任务与安全闸等完整 EGIA 启动配置。IsOS 通过 `EGIA_ETC` 和 `--etc` 传入末级目录下绝对的 `egia/` 路径。EOS 完整 gRPC 网关尚未就绪时，用 EGIA setup 桩测试此流程，保持 EGIA 运行时禁用。重建选定程序或更改已安装默认配置后，显式以相同保存修订 Apply 以选择新生成版本，再 Restart。失败的 Apply 或 Run 报告脱敏的 `generated_*` 错误码，活动部署不变。密钥值只放在管理器环境中；只配置 `env_from` 名称，并检查任何计划启动子进程缺少名称时，在停止活动子进程之前就失败。值只在 spawn 时传给指定子进程。生成文件只留在 IsOS 私有数据根目录，不写入任何源码仓库。[配置和预检契约](../interfaces/configuration_cn.md#p04-生成配置与启动预检isos-已实现)列出冻结字段和错误。

若重启时新子进程启动失败，IsOS 会停止本次启动，并按先前的进程配置重新启动先前的运行中修订。若先前的子进程也无法恢复，状态会显示部署已降级及重启错误。

**P05 读取器特性预检——目标，尚未实现：**选择 EOS、可选 EGIA 构建及精确 MuJoCo 包后，运行 `isos version check --json`。比较各启用核心的所选程序、请求的图版本、完整节点类型及必需特性与公布的支持集合。Validate／Apply 对草稿重复检查；Run／Restart 在停止任何子进程前对保存修订重复检查。即使产品版本较新，缺失 `@v0.1` 类型、`mujoco-child-v2` 声明、EGIA runtime 的 `managed-config-v1`／`eos-gateway-client-v2` 探测或已验证包 pin 仍会阻塞。对 EGIA，应先选定已有解释器及已安装模块树；能力探测不得创建虚拟环境或运行 `uv sync`，IsOS 在 Restart 前重查解析出的解释器和代码来源。恢复兼容的所选构建并再次 Apply；只有报告给出一个精确已验证发行版时才采用升级建议。现有命令的检查范围较窄；[P05 契约](../interfaces/reader-features_cn.md)规定目标诊断和失败不停止行为。

### 迁移已保存的旧图

编辑已应用的 `1.0.0` 图之前，先从 Graph 或 `GET /api/isos/v1/node-graph` 读取保存修订号。整图已映射且安全时，还会显示只用于检查的 `legacy_projection`：旧类型 ID 保持不变，`mappings` 列出候选替代类型，`defaults` 标记仅供展示的默认值。应编辑原始 `graph` 并显式迁移；把投影当作更改图提交会被拒绝且不写入。未映射的旧 control 类型保持只读，并在 Run 之前报告精确的 `/node_type` 路径。预览会报告精确类型替换、完整的 `0.1.0` 建议图及阻塞原因，且不改变保存字节或运行中的子进程：

```sh
isos config migrate --preview --revision <saved-sha256>
```

检查建议结果并解决归属方 reader 或软件包问题，再用**预览给出的摘要**和两个修订号提交：

```sh
isos config migrate --revision <saved-sha256> --proposed-revision <preview-sha256>
```

来源或建议摘要过期时须重新预览。`isos status --json` 随后显示新的保存修订号；运行中修订号在显式 Restart 前保持不变。未匹配的旧类型保持只读，须按[迁移指南](../maintenance/migration_cn.md)显式修复。
