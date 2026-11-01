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

4. Apply 后执行 `isos run`，它保持前台运行。在另一终端先用 `isos status --json` 查看保存／活动修订、进程就绪和最近错误，再用 `isos logs all --level info` 查看启动顺序。当前 IsOS 启动行表示启动尝试，状态才确认就绪。MuJoCo 或可加载适配器的软件包加载故障先读 `isos logs packages --level warn`：`package_id`／`package_version` 标识固定发行版，`run_id` 和 `correlation_id` 关联管理器预检失败；再查看命令错误确定修复方法。EOS 与 MuJoCo 来源解释子进程启动。特定故障用 `isos logs eos --level warn` 或 IsosUI Logs 面板调查。日志位于私有数据根目录的 `logs/<deployment>/`；路径、过滤、轮转和游标见[配置与日志](../interfaces/configuration_cn.md)。**P06 首次使用：**始终用 `isos status --json` 检查实时就绪。Validate、Apply 或 Run 预检失败时，查管理器会话 `run_id` 和请求 `correlation_id` 下首条 `packages` 或 `isos` 错误；安全类型化字段记录精确软件包 pin 和可用的已验证收据摘要。Start 获准后，用新分配的部署运行 ID 查一条 `deployment_starting`、所选 EOS 子进程的 `eos_ready` 及启用时 EGIA 归属方的就绪事件，再查 `deployment_ready`。用 `isos logs eos --level warn` 和同一运行／关联 ID 查子进程状态变化及终结停止事件。EOS 原始仿真诊断留在独立私有 `owner/eos/` 目录。同一失败请求的包加载详情与预检摘要共享关联 ID；首次 Start 失败后修复重试会使用新的运行 ID。无效图指针从安全事件中省略，但失败摘要仍保留。[统一事件契约](../interfaces/events_cn.md)定义 JSONL 边界及来源映射。P07 已增加跨轮转的首事件游标；查看 status 后使用 `isos logs all --current` 读取固定的首事件与后续保留日志。

5. Ctrl+C 停止附着的部署。`isos deployment stop` 仅停止图；`isos stop` 也停止独立服务、UI、API 和管理器。重复停止会成功。停后用 `isos status --json` 确认已停止；若停止异常，再查看警告与错误日志。干净停止无需大量周期性消息。

运行前会重新校验精确的图修订、已配置程序、软件包、场景和资源。EOS 启动并持有 MuJoCo 子进程。安装包、修改程序路径或编辑草稿都不会改变运行中的部署；需显式 Apply 和 Restart。参见[软件包命令](packages_cn.md)、[图字段](../interfaces/node-graph_cn.md)及[独立服务契约](../interfaces/services_cn.md)。

对于已应用的 `0.1.0` 图，Run 会在启动前将按修订保存的生成文件与新的归属方 render 比较。如果配置的 EOS 或 EGIA 程序发生变化，`isos init` 会报告过期的启动输入并保留不可变快照。恢复固定的程序，或创建新的图修订并 Apply，然后再运行。

**P04 生成配置首次使用：**选择已安装的 EOS 和 EGIA 发行版，将 `paths.egia_defaults_dir` 和 `paths.egia_factory_defaults_dir` 分别设为该 EGIA 发行版的组件与工厂默认目录，并验证精确的 MuJoCo 软件包固定版本，然后 Validate 并按保存修订 Apply。IsOS 发布为该精确修订选定的私有生成版本。检查 `data/generated/<deployment>/selection.json` 中保存修订对应的条目、持久的 `v2/required/<revision>.json` 标记及选中目录的 `v2/<revision>/<generation>/manifest.json`；摘要须与保存图和所选已安装输入匹配。P04 Apply 后若选择记录缺失，Run 会阻止启动，不会退回旧 G03 快照。末级目录包含 EOS `graph.json`，启用 EGIA 时还包含模型、提示、任务与安全闸等完整 EGIA 启动配置。IsOS 通过 `EGIA_ETC` 和 `--etc` 传入末级目录下绝对的 `egia/` 路径。EOS 完整 gRPC 网关尚未就绪时，用 EGIA setup 桩测试此流程，保持 EGIA 运行时禁用。重建选定程序或更改已安装默认配置后，显式以相同保存修订 Apply 以选择新生成版本，再 Restart。失败的 Apply 或 Run 报告脱敏的 `generated_*` 错误码，活动部署不变。密钥值只放在管理器环境中；只配置 `env_from` 名称，并检查任何计划启动子进程缺少名称时，在停止活动子进程之前就失败。值只在 spawn 时传给指定子进程。生成文件只留在 IsOS 私有数据根目录，不写入任何源码仓库。[配置和预检契约](../interfaces/configuration_cn.md#p04-生成配置与启动预检isos-已实现)列出冻结字段和错误。

若重启时新子进程启动失败，IsOS 会停止本次启动，并按先前的进程配置重新启动先前的运行中修订。若先前的子进程也无法恢复，状态会显示部署已降级及重启错误。

**P05 读取器特性预检：**选择 EOS、可选 EGIA 构建及精确 MuJoCo 包后，运行 `isos version check --json` 并检查各启用核心的诊断，然后按已保存修订 Validate 和 Apply 草稿。Run／Restart 在替换子进程前重查选定图、程序身份、必需特性及 P04 生成版本。即使产品版本较新，缺失 `@v0.1` 类型、`mujoco-child-v2` 声明、EGIA runtime 的 `managed-config-v1`／`eos-gateway-client-v2` 声明或已验证包 pin 仍阻止准入。对 EGIA，先通过 `EGIA_PYTHON` 或 `ISLAND_PYTHON` 选定已有解释器和已安装模块树；不启动服务的能力探测不会创建虚拟环境或运行 `uv sync`。IsOS 记录解释器路径、解析后目标、摘要及代码清单，Restart 前重新核对。恢复兼容的所选构建，再对相同修订 Apply。只有存在与图精确匹配的已验证发行版时，`compatible_upgrade` 才不为 `null`。[P05 契约](../interfaces/reader-features_cn.md)规定诊断字段和停止前失败行为。

## R03 托管控制授权重放

**R03 授权交接已验证。** 对 `0.1.0` 图 Validate 和 Apply 后，从干净的 `ISOS_HOME` 启动托管部署。检查 `isos status --json`：`active_revision` 必须等于保存图精确字节的 SHA-256，`run_id` 必须标识本次启动。IsOS 必须显式选中并验证 schema-2 V2 生成代，在 `run/control-grants/<deployment>/<run_id>/` 下创建一个私有运行授权包；G03 回退不够。只检查文件名、权限以及验证文件的运行/修订，不打印授权内容，也不把它放进命令行、浏览器或日志。必需的已选 EOS 运行时接收 `EOS_CONTROL_GRANTS_FILE`；仅**已选** EGIA 接收 `EGIA_EOS_CONTROL_GRANT_FILE`，仅**已选**受监管 bridge 接收 `AETHER_BRIDGE_CONTROL_GRANT_FILE`。EOS 加 config 的部署在没有 EGIA 或 bridge 时仍有效；config 不接收授权。[所有者契约](../interfaces/control-grants_cn.md)规定精确字段和方法范围。

使用 EOS 发现元数据，把 `eos-active-revision` 与 IsOS 的 `active_revision` 比较；`eos-projection-sha256` 是独立摘要。若已选择，托管 EGIA gRPC 运动使用自己的角色授权和活动修订。操作员授权允许 bridge `primitive_typed` 运动及受监管的 gRPC stop/recovery，不允许 gRPC 运动；任何授权都不从浏览器获取。丢失响应时按原操作 ID 查询。幂等 Start 保留本次运行授权；显式 Restart 即使图字节未变也轮换两个授权。R06 在子进程意外退出后阻止自动重启，直到依赖停止得到确认或完成归属方恢复。`deployment_starting` **之前**的拒绝须用请求关联号在传统 `isos` 日志来源和状态中查找管理器会话的 `deployment_preflight_failed`；`isos logs all --current` 此时可能仍显示以前准入的运行。候选运行开始后，才用 `isos logs all --current` 检查其首条就绪或故障事件。确认没有凭据或 bearer 文本。授权包缺失、不安全或过期时，必须在 EOS 可写就绪前失败。最终本地干净 home 重放已对已选 EOS 候选验证 IsOS 签发与传递、真实受监管 gRPC 运动及结果查询，并在 Restart 时分别断言 EGIA 与操作员授权的新旧值不同（见[证据](../design/control-grants_cn.md#集成边界与测试)）；在 [EOS #315](https://github.com/Island-Team/Island-eos/pull/315) 的 `dbd2e2c39a54236786577ace424c30e418ff3926` 和 [IsOS #52](https://github.com/Island-Team/isos/pull/52) 的 `df3c1f5692c9348c1cfb5b2ecb8b1abfa9fa82f2` 合并后，主仓库重放也已通过（见[命令与日志](../design/control-grants_cn.md#集成边界与测试)）。R05/R06 后续验证 MuJoCo 实体 fence 与安全停止；较广的 I4 生命周期门槛仍未关闭。

已选 EOS 的 `--capabilities` 必须在 Run 或 Restart 前包含精确的 `managed-control-grants`；旧二进制在读取器预检中被拒绝，现有子进程保持运行。

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

## P07：查看首条事件和后续日志

首次准入运行前，使用 `isos logs all --current --level info --lines 100` 或 `GET /api/isos/v1/logs?view=current&source=all&level=info`。管理器会话范围会固定首条软件包或 Apply 故障。运行准入后，同一请求改为返回该部署运行的首条就绪或故障事件；即使来源／级别／节点／关联筛选不匹配，锚点仍独立显示。锚点待定时轮询 `next_cursor`。停止或失败后仍查看最后准入运行的终结日志；新运行准入时旧游标返回 `log_run_changed`。轮转保留期间游标连续；过期、收集中断或锚点不可用时显示 `diagnostics`，然后继续使用新游标。每次先查 `isos status --json` 的实时状态；日志页只解释历史。遇到 `run_id_state:unavailable` 或 `log_scope_unavailable` 时显示有类型故障，不能呈现管理器会话页。详见[当前运行日志游标契约](../interfaces/log-cursors_cn.md)。
## R04 原生技能选择目标

**IsOS 选择切片已通过独立审查；发布前仍须完成 R04 最终合并版本的验收。** 安装或链接一个精确且通过 P01 验证的 `kind:"skill"` 发布，其中含原生 `.skill.so`。在保存的 `0.1.0` 草稿中，仅给 `isos/process/component/eos` 增加 `settings.skill_libraries` 数组；该节点 role 为 `eos`，解析程序为 `aether-runtime`。每项列出精确 `{id,version}` pin 和包内普通文件相对路径。`bridge`、`config` 虽也有 EOS role，但不可设置此字段。不要编写 `eos.control.skill@v1.0` 或图技能步骤。[R04 所有者契约](../interfaces/skill-selection_cn.md)列出精确结构和界限。

运行 `isos version check --json`、Validate，再以当前保存修订 Apply。列表非空时，选中的 EOS 程序须声明 `host-skill-abi-v2`。Apply 后只检查选中 schema-2 `manifest.json.files` 中 `isos/skill-selection.json` 的条目、`active_revision`、精确包 ID 和库路径，不修改私有哈希或文件。Run 后用 `isos status --json` 与 `isos logs all --current` 检查 EOS catalog 就绪或安全的准入失败，并关联相同 run/correlation ID。只有 EOS 接收 `EOS_SKILL_SELECTION_FILE`；bridge、config、EGIA 和浏览器均不接收。通过类型化 EOS SkillService 发现已准入技能，只调用声明的身份，并观察终态结果及有界事件。安装包或修改草稿不会启用技能；同 revision 的包变更需要显式重新 Apply 和 Restart。

对于缺少 feature、符号链接或变更的链接库、错误的选中 generation 或过期 revision，确认拒绝发生于停止任何活动子进程之前；旧 PID、active revision、run ID 与 R03 grant 均保持。用 correlation ID 查看首次 manager-session 错误事件。EOS 在候选运行中、guest handler 调用前拒绝无效 ABI 或描述符。handler 请求未声明 primitive 时，Broker 在 Dispatcher release 前拒绝。查看该运行的 EOS 事件，再 Stop 并确认 EOS 回收 worker。此重放验证准入边界；EOS worker 测试另行验证受限执行。完整命令与输出保存在 `logs/test/<run-id>/`。

**R04 事件准入已合并；最终合并版本的跨所有者重放仍未完成。**使用从 EOS 精确提交 `192fbfa6fbed1e9b6a20dd8dc95501625b02d081` 构建的程序，先在 EOS 私有归属日志中核对所选 runtime 的 `eos_skill_execution` 排队、运行及终结记录，及可能的 `eos_skill_stop_unconfirmed`。跨归属回放继续使用 `isos logs all --current` 重放：status 确认实时就绪；首条已接纳技能失败可成为固定锚点，后续生命周期记录使用同一部署运行及 `skill_` 关联。先查看未筛选锚点，再按关联筛选。用 `next_cursor` 跨保留的 `events.log` 轮转，再测试过期游标及 `log_collector_gap`；封印锚点须保留。Stop 后分别查看最后运行与停止故障。无效候选 pin 须在停止旧运行前的预检中拒绝。见[精确子进程事件策略](../interfaces/events_cn.md#r04-原生技能子进程事件实现复核)与[R04 回放案例](../../../tests/fixtures/r04_skill_events/README_cn.md)。此 IsOS 切片已通过所选子进程管道准入这两个事件码；完整 EOS SkillService 跨归属回放仍是独立门禁。

旧运行仍在服务时，Restart 建立候选 R03 verifier，并对已选 EOS 执行私有目录预检，统一硬期限为 90 秒。按请求 correlation ID 查看 manager-session 的首条 `deployment_preflight_failed`，识别 `skill_catalog_preflight_rejected`、`skill_catalog_preflight_invalid_output`、`skill_catalog_preflight_timeout` 或 `skill_catalog_preflight_cleanup_unconfirmed`。无效 ABI、缺失静态 primitive binding 或超大目录必须保留旧 PID、活动修订、run ID、grant 与 run scope；只有确认候选 grant 撤销和探测进程回收，才能报告干净拒绝。临近期限的超时或 grant 撤销失败不得停止旧进程。`isos logs all --current` 可能仍指向旧运行，不能用它推断本次首个错误。

探测成功后，manager 在停止旧子进程前重检精确选中字节和 grant。EOS 在实际启动时重复 ABI 和就绪准入，因此探测与启动间的库或程序变化会关闭准入；停止旧进程后的失败走现有恢复路径。先读 `isos status --json` 的实时状态，再读候选 run 的首条就绪或故障事件，然后按 correlation ID 筛选 EOS 日志。此重放不证明 R05 的实体 release fence 或 R06 的 actuator 停止与模拟器回收次序。完整命令、子进程 PID 与输出放在 `logs/test/<run-id>/`。
针对 `b5bf113adf7760efc8f98114d0d5aaa090fac62b` 的真实 EOS asset 选择重放已通过：双 asset 原生发布接受 `ISOS_GPU=nvidia` 与 `ISOS_ROS=jazzy`，并在启动前拒绝缺失选择器或错误 ROS。候选探测现在仅传这些有效选择器，遵循 EOS 子进程相同的图中环境及 `env_from` 优先级。清理标记写入故障注入也确认后续替换持续被阻止。参见[复核修复证据](../design/skill-selection_cn.md#候选预检事务目标设计)。

## R06 依赖停止目标

EOS 或其他提供者意外退出时，管理器自己的 daemon 轮询会立即停止并回收其余依赖进程，无须打开状态页。该运行报告 `unknown` 并阻止 Restart。归属方评审恢复前，先查看实时终结结果和 `deployment_stop_unconfirmed` 事件。

**实现中，尚待验证。** Run 前检查所选 EOS 构建有 `managed-stop-v1`；该 token 只声明支持，不能证明本次运行已停止。执行 `isos deployment stop`、附着运行的 `isos run` Ctrl+C 或 `isos stop` 后，先用 `isos status --json` 检查实时 `terminal.outcome` 与运行 ID，再用 `isos logs all --current` 查同一运行的终结事件。`confirmed` 要求选中 EOS 子进程原始 stdout 的严格 `eos_terminal_stop` 收据与运行／修订／PID／后端／部件匹配，证明底盘和机械臂、MuJoCo 组排空及回收、socket 清理，且 EOS 以零码退出；空进程列表、PID 消失或 `eos_stopped` 日志不足够。EOS 归属方停止硬界限为 15 秒；IsOS 缺省在 30 秒总期限内预留 20 秒。收据过晚、无效、缺失或明确未知会使部署降级并阻止 Restart，直到归属方评审的恢复 fence 证明安全。即使实时确认，管理器重启后也降级为未知，因为 IsOS 只持久保存未知停止意图。[依赖停止契约](../interfaces/dependent-stop_cn.md)规定精确解析字段、原因码与期限配置。真实归属方 R06 重放通过前，当前程序仍未合格。
