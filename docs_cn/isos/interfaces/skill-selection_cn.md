# R04 已验证 host 技能选择

[English](../../../docs/isos/interfaces/skill-selection.md) · [设计](../design/skill-selection_cn.md) · [首次使用](../usage/get-started_cn.md#r04-原生技能选择目标) · [固定用例](../../../tests/fixtures/skill_selection/README_cn.md)

**R04 所有者契约：IsOS 选择切片已通过独立审查；最终合并版本的验收仍待完成。** IsOS 拥有已保存图、P01 精确软件包 pin、P04 选中 generation 与 EOS 子进程启动输入。EOS 拥有 ABI v2 描述符准入、受限 worker、能力 Broker、Dispatcher 与 SkillService。安装软件包本身不启用技能。这里仅选择原生库，不定义图中的技能步骤、WorkflowService 或新的执行器通道。[EOS host ABI 契约](https://github.com/Island-Team/Island-eos/blob/refactor%28v2%29/runtime-main/docs_cn/eos/interfaces/skills/native/host-abi_cn.md)定义 guest 执行。

## 图中的选择

只有位于 `isos/process/component/eos`、`settings.role:"eos"` 且解析到 `aether-runtime` 的已选 `isos.process.component@v0.1` 节点可以有可选 `settings.skill_libraries`。`bridge`、`config` 和其他 EOS-role component 虽也有 `role:"eos"`，仍在其 `/settings/skill_libraries` 指针失败。G06 的 `@v0.1` component setting catalog 声明此字段；所有者检查进一步限定 runtime 节点。字段缺失或 `[]` 表示不启用外部 host，也不传 `EOS_SKILL_SELECTION_FILE`。非空条目只能是 `{ "package": { "id": "namespace/name", "version": "x.y.z" }, "library": "lib/island/skills/example.skill.so" }`；拒绝未知字段。`package` 是 P01 精确 pin；`library` 是软件包内 POSIX 相对路径，不是 glob、目录、命令或 URL。图不能指定 host 程序、worker 参数、handler、callback、步骤或特权能力。

| 字段 | 类型 / 单位 | 用途与校验 |
| --- | --- | --- |
| `settings.skill_libraries` | 数组 / 条目 | 可选，最多 16 项，仅限 EOS `@v0.1` component。生成文件按 `(package.id,package.version,library)` 排序；重复元组失败。保存图的字节和数组顺序仅在显式 Apply 时更新。 |
| `package.id` | 字符串 / 软件包 ID | 必填，规范 `namespace/name`；选中技能最多 96 个 ASCII 字节，P01 精确验证。 |
| `package.version` | 字符串 / 发布版本 | 必填，P01 接受的精确版本；选中技能最多 64 个 ASCII 字节且至少三个点分段，不使用 latest 或范围。 |
| `library` | 字符串 / POSIX 相对路径 | 必填 UTF-8，1–256 字节；分量不得为空、`.` 或 `..`，不得有反斜杠或 NUL，后缀为 `.skill.so`。目标必须是 1–64 MiB 的非符号链接普通文件；不扫描目录或递归发现。 |

P01 图 pin 收集器把每个选中 `package` 计入，并使用精确 `/cores/isos/layers/process/groups/component/nodes/<id>/settings/skill_libraries/<index>/package` 指针。IsOS 对每项重新取得 P01 `VerifiedPackage`，要求发布 `kind:"skill"`，检查 payload 根、文件身份与 SHA-256；变更的链接 payload 失败。只有明列的库被选中。EOS 随后检查所有库中重复的技能身份。旧 `1.0.0` 图和 `eos.control.skill@v1.0` 不因此获得执行行为。

所选技能门禁在 Validate／Apply 和 Run／Restart 停止活动部署前，均于数组索引下的 `package` 指针执行上述 96／64 字节与版本语法限制。P01 已验证但版本超过 64 字节的链接发布以 `skill_selection_invalid` 拒绝，且不分配候选授权或探测进程。[R04 事件设计](../design/events_cn.md#r04-所选技能事件准入设计)与[夹具](../../../tests/fixtures/r04_skill_events/README_cn.md)覆盖这一边界；通用 P01 包语法不变。

对于选中的技能发布，IsOS 还检查打开的库文件、P01 `receipt.json` 与 `release.json`，以及已验证 payload 树中的每个普通文件。每个文件都必须由 IsOS 的有效 UID 拥有，且 group/other 均不可写（`mode & 0o022 == 0`）。选中库另需满足 1–64 MiB 限制；元数据与 payload 文件沿用 P01/EOS 大小限制。不安全文件在 Apply 发布或 Run/Restart 停止旧进程前失败；EOS 暂存前重复相同的所有者与权限检查。即使未选中的 payload 文件可被其他用户写入，整个技能包也会被拒绝。

G06 catalog 在 `isos.process.component@v0.1` 上以新增的 `data_type:"object_array"`、`min_items:0`、`max_items:16` 和 JSON Schema 对象 `items_schema` 如实描述该字段。item schema 为 `type:"object"`、`additionalProperties:false`，必需 `package` 与 `library`；`package` 也是不允许多余字段且需要字符串 `id`、`version` 的对象；`library` 是 1–256 字符的字符串。EOS registry 只接受有界的本地 schema 子集（`type`、`properties`、`required`、`additionalProperties`、`minLength`、`maxLength`、`pattern`），最多 8 KiB、深度 3，拒绝 `$ref` 与未知关键字。IsOS 进一步检查字节/路径、精确 owner 节点和 P01 pin。无新字段时，旧 setting metadata 序列化不变；`@v1.1` catalog 和旧图不变。下游 U01 表单获得真实的 object-array 描述，而非伪装成字符串。实现还需扩展 IsOS `ProcessSettings` 和 G06 聚合 catalog 适配器。

IsOS 拥有并发布同时含已有 `env_from` 和新 `skill_libraries` 的 `@v0.1` component 描述符，使用 EOS 新增的通用 `SettingType::ObjectArray` 支持。历史 EOS `@v1.1` catalog 描述符只是兼容输入，不拥有新的 IsOS 类型。旧 EOS 的 fallback clone 仅能为无技能选择的图提供 `env_from`，不得合成 `skill_libraries` 或准入非空选择。P05 仍检查选中 runtime 程序的精确 `host-skill-abi-v2` feature。

## 选中 generation 与交接

非空列表使 Apply 在不可变 P04 `v2/<active_revision>/<generation>/` leaf 中增加 IsOS 编写的 `isos/skill-selection.json`；schema-2 `manifest.json.files` 恰好列出该文件一次，`manifest.json.packages` 列出相关 pin。文件为 `0600`，父目录为 `0700`。紧凑、排序后的 JSON 字段恰好为 `schema_version:1`、`active_revision` 和 `libraries`。每个条目恰好含 `package_id`、`package_version`、`package_root`、`payload_root`、`receipt_sha256`、`release_sha256`、`payload_sha256`、`library` 与 `library_sha256`；所有摘要为 64 位小写十六进制 SHA-256。`package_root` 是 P01 私有元数据 store 的绝对路径；`payload_root` 是 P01 已验证 payload 的绝对路径，链接包可位于外部。它们只能由 IsOS 的已验证句柄产生，不能由图或浏览器提供。

完整序列化文件最多 65,536 字节（64 KiB），与 EOS 的有界选择读取器一致。已验证链接包路径过长等情况导致文件超限时，IsOS 在 Apply 发布或 Run/Restart 替换前于 EOS 的 `settings.skill_libraries` 指针拒绝。

`active_revision` 等于保存图字节及 P04 manifest 的 `revision`，不同于 EOS projection 摘要。文件不含 `generation`，因为 P04 generation 本身包含最终文件哈希；leaf 路径和 manifest 提供 generation。空列表不生成该文件，选中 leaf 中意外出现该文件应失败。只有列表非空时，选中的 `aether-runtime --capabilities` 必须声明精确 `host-skill-abi-v2`；缺失则在 Apply、Run/Restart 停止旧子进程或执行 guest 前返回 P05 `reader_feature_unsupported`。IsOS 只向选中的 EOS 子进程注入 `EOS_SKILL_SELECTION_FILE=<选中文件绝对路径>`，其他子进程清除继承值；作者设置的冲突值或保留参数在停止旧运行前失败。R03 的角色 grant 独立存在。

Run/Restart 在停止旧子进程之前重新检查保存图、V2 selection 与 marker、完整 manifest、P01 pin、库摘要和 EOS feature。IsOS 实现候选现在于停止旧子进程前检查所选 ABI；独立实现复核和真实 EOS 重放尚待完成。缺失、变更、未列出、不安全或 revision 错误的选择失败，并保留当前 PID、run ID、active revision 与 grant。G03 和旧 flat 快照不能启用该选择。相同图 revision 的包或二进制变更需要再次 Apply 来选择新 generation，再显式 Restart 才生效。自动 EOS 子进程重启沿用同一次运行的文件和 grant；Stop 在子进程回收后撤销 R03 grant。

EOS 在 worker 查询前独立验证 manifest、文件、revision、摘要和路径。对于链接包，EOS 以 nofollow 打开精确文件，从同一打开句柄计算摘要并复制到 EOS 私有只读暂存文件；受限 worker 只加载该副本。无效 ABI、描述符或不支持的已声明 callback 类别在 handler 调用前失败；handler 运行时请求未声明 primitive 则由 Broker 在 Dispatcher release 前拒绝。IsOS 与 EOS 的两次检查均不可省略。

## 候选 ABI 预检（IsOS 实现候选）

IsOS deployment manager 是调用方，选中的 EOS `aether-runtime` 是被调用方与 ABI 检查所有者。传输是 manager 直接子进程和 stdout 上一行 JSON。非空受监管技能选择完成 P01/P04/P05、进程规格检查后，必须在 `managers.remove(name)`、停止旧子进程、发布候选 run scope 或 `deployment_starting` 前执行。空选择沿用现有路径，不获得可执行 host 目录；未经验证的包、图或能力不能绕过此门槛。

IsOS 分配候选 run ID 和 R03 grant bundle，但不发布候选 run scope。用**同一个已选且已验证**的 `aether-runtime` 执行 `--skill-catalog-preflight --profile <selected-v2-leaf>/eos/graph.json --run-id <candidate-run-id>`，附带实际选中的 `--node-types` 参数。在清理继承值后的 manager 环境中，仅传 `EOS_CONTROL_GRANTS_FILE=<candidate-verifier>`、`EOS_SKILL_SELECTION_FILE=<selected-v2-leaf>/isos/skill-selection.json` 及所选 EOS 子进程实际会收到且已验证的有效 `ISOS_GPU`/`ISOS_ROS` asset 选择器，不传 bearer grant。图中 `settings.env`、被移除的 secret-source 名称和已解析的 `env_from` 值遵循普通子进程启动优先级；停止旧子进程前及实际启动时再次检查选择器。私有命令拒绝 listener、socket、journal、adapter、backend 和 scene 等启动选项。只能在受限 inspection worker 中运行库的静态构造函数和 ABI query；不能创建 listener、模拟器、adapter、Dispatcher、guest `execute` handler 或发布 ready。EOS 检查精确保存修订、schema-2 leaf/manifest、候选 verifier/run 绑定、P01 包和库字节、ABI v2 描述符、声明的 Broker 覆盖、每个声明 primitive 的静态部署 binding、重复身份与完整编码的 `ListSkillsResponse` 3 MiB 上限。成功前回收所有检查 worker 并清除暂存。EOS [私有预检契约](https://github.com/Island-Team/Island-eos/blob/cf48e439c88d7938f5d5e9cf839b066883359d36/docs_cn/eos/interfaces/skills/native/host-abi_cn.md)规定封闭结果和启动共用的纯数据就绪校验。实时健康、终端 stop fence、adapter 与 Dispatcher 仍由启动或调用阶段检查；此预检不声称完成 R05/R06 的实体停止。

stdout 至多 16 KiB；stderr 仅限收集至 16 KiB 后丢弃。IsOS 只接受**一行** UTF-8 JSON，拒绝重复或未知键。成功必须退出码 `0`，并精确包含 `schema_version:1`、`ok:true`、`active_revision`、`generation`、`run_id`、`selection_sha256` 和 `libraries`。修订、generation、摘要均为小写 SHA-256；run ID 等于候选身份。`libraries` 按选中文件顺序含 1–16 项，每项精确包含 `package_id`、`package_version`、`library`、`library_sha256`、`abi_version:2` 与 1–256 的 `skill_count`。IsOS 逐项比对保存修订、选中 leaf、候选 run、选中文件摘要和有序已验证 pin/库摘要。失败必须以非零退出码和精确四字段 `{ "schema_version":1, "ok":false, "stage":"abi", "code":"abi_descriptor_invalid" }` 返回；`stage/code` 只允许下表组合。message、路径、schema、grant、stderr 与 guest 文本均不是协议数据。

| EOS 阶段 | 允许的 EOS 代码 |
| --- | --- |
| `input` | `preflight_arguments_invalid`, `preflight_grants_invalid`, `preflight_profile_invalid` |
| `selection` | `selection_invalid`, `selection_revision_mismatch`, `selection_manifest_invalid`, `selection_digest_mismatch`, `selection_handoff_incomplete` |
| `package` | `package_invalid`, `package_changed`, `package_unsafe`, `library_changed` |
| `elf` | `library_not_elf`, `library_dependency_unsafe` |
| `worker` | `worker_spawn_failed`, `worker_sandbox_failed`, `worker_inspection_failed`, `worker_timeout`, `worker_reap_failed` |
| `abi` | `abi_library_invalid`, `abi_descriptor_invalid`, `abi_schema_invalid`, `abi_capability_unowned` |
| `catalog` | `catalog_duplicate_identity`, `catalog_capacity`, `catalog_broker_unavailable`, `catalog_binding_unavailable`, `catalog_list_oversize` |

IsOS 对包含包树读取、暂存和库检查的整次探测设置**硬 90 秒墙钟期限**。到期后终止探测进程组，再单独进行有界回收；迟到的成功不得覆盖超时。信号退出也按超时处理。P01 在包链接或安装时拒绝发布版本 `1`。P01 有效的 65 字节版本可链接，但 IsOS 在 Apply 发布或候选 grant/探测分配之前，于选中图的 `/package` 指针以 `skill_selection_invalid` 拒绝；同一门槛拒绝超过 96 字节的选中包 ID。该检查与 EOS 选中 pin 上限一致，维护事件安全 pin 边界。有效非零 `ok:false` 对应 `skill_catalog_preflight_rejected`；缺失、多行、超大、无效、不匹配的输出或退出码与 `ok` 矛盾对应 `skill_catalog_preflight_invalid_output`；期限或信号对应 `skill_catalog_preflight_timeout`；无法确认探测进程或候选 grant 清理对应优先的 `skill_catalog_preflight_cleanup_unconfirmed`。四个固定 IsOS schema-1 诊断使用 `stage:"preflight"`、空 `path`，归入 `deployment_preflight_failed`。日志只可包含安全的 revision、generation、候选 run/correlation ID、有效的 EOS stage/code 与 IsOS code；不得包含探测输出、stderr、私有路径、原始 schema、bearer grant 或库字节。候选 run scope 尚未发布，首个故障属于 **manager-session** 游标；`isos logs all --current` 可能仍指向旧运行。

任一探测失败或后续停止前重检失败，IsOS 都须通过返回 `Result` 的显式路径撤销候选 bundle，并确认探测进程组已清理。当前 `Supervisor::stop()` 与 Drop 只打印撤销错误，不能证明清理。撤销失败报 `skill_catalog_preflight_cleanup_unconfirmed`，旧 PID、run ID、活动修订、grant 与 run scope 均保持不变，下一次替换前需要操作员修复。拒绝候选不启动恢复运行。`ok:true` 后，IsOS 在停止旧子进程和启动候选前，立即重检保存图修订、选中 generation 完整清单、精确程序字节、候选 grant 与 P01 收据/发布/payload/库字节。探测结果不能作为启动授权。新 EOS 进程在 ready 前以新暂存字节重复受监管准入、ABI 检查和共享静态就绪校验；同一运行中 EOS 自动重启也重复启动准入。仅在旧运行**确认停止**后发现变化时，才走现有恢复路径并报告候选启动失败。

## 成功探测后的替换停止与授权撤销

**目标设计——已合并 R04 master 尚未实现。**本设计处理[跨仓库实现 TODO](https://github.com/Island-Team/Island-eos/blob/refactor%28v2%29/runtime-main/docs/island/implementation-todo.md) 中的旧 Stop unknown 缺口。IsOS R06 [#57](https://github.com/Island-Team/isos/pull/57) 已有可检查的旧运行停止 terminal，但其 unknown Restart 路径仍将未发布候选 bundle 留给隐式清理。本 R04 切片须增加显式且可检查的候选撤销，并与 R06 组合重放；任一分支单独不能证明此组合路径。

`ok:true` 和停止前复核通过仍不授权启动候选。IsOS 必须取得 R06 对旧准入运行的可检查终结结果，其中包括 EOS 实体停止证明、直属子进程组清空和旧运行 grant 撤销。不能从 `Supervisor::stop()` 返回的 ID、消失的 PID、日志或探测成功推断确认。只有旧结果为 `confirmed` 且旧 grant 撤销已确认，才能发布候选 run scope、启动候选子进程或恢复运行。保存图字节不变；在结果解决前，状态与持久范围保留旧 P07 run ID 和活动修订。

旧结果若因任何 R06 原因（包括 `control_grant_retire_failed`）为 `unknown`，IsOS 须经可检查的候选 grant API 撤销**未发布候选**的 bundle。发布旧运行的 R06 `terminal:{outcome:"unknown",reason_code,...}` 和带 Restart 请求关联号的 `deployment_stop_unconfirmed`，Restart 返回带类型的 `deployment_stop_unconfirmed` 停止故障。状态为 `degraded`，不得呈现为正常运行或安全停止；旧子进程即使仍存在也不能标为健康。旧运行持久 unknown stop intent 是安全标记。候选没有 P07 发布、`deployment_starting`、ready 事件或授权。重试、幂等 Start、自动子进程重启和管理器重启均不得从此状态启动任何一代；需按 R06 停止契约和现有[运行／停止算法](../design/system_cn.md)执行所有者复核的实体恢复。同一管理器内重试保留原 R06 原因；管理器重启后，同一个旧 P07 run ID 从有效停止记录恢复为 `unknown`、`stop_interrupted`，或从缺失／无效记录恢复为 `unknown`、`stop_result_persist_failed`；历史日志不能恢复早先原因或 confirmed。停止可能部分或全部回收旧 PID，不能声称旧 PID 集合未变。

旧 Stop 为 unknown 时仍须检查候选 bundle 撤销。若撤销失败或无法验证，候选清理错误优先返回 `skill_catalog_preflight_cleanup_unconfirmed`（`stage:"preflight"`、`path:""`），旧 R06 terminal 和 `deployment_stop_unconfirmed` 事件仍记录实体停止原因。IsOS 写入现有私有 `run/<deployment>.skill-candidate-cleanup-unconfirmed` 标记；主写入失败时使用 data root 备用标记和当前 manager 内存阻断；若两次写入均失败，遗留候选 grant 目录（其中可能仍有未撤销的 grant 文件）仍阻断重启后的替换。标记绑定部署，不含 bearer 或私有路径；候选 grant 目录保留候选 run 身份。只有操作员证实探测组不存在、候选 bundle 已撤销且旧运行 R06 恢复 fence 满足后，才可清除；API 重试不得自动清除。第二次 Restart 不分配新候选 grant，也不再次探测 EOS，而返回相同的清理或停止诊断。

Restart 错误必须区分来源：旧 Stop 不确定是带 R06 原因码的 IsOS 停止结果；候选清理不确定是 IsOS 预检诊断。二者均不可伪装为 EOS ABI 拒绝或 P01 包错误。schema-1 HTTP 错误 envelope 和 CLI 非零退出携带安全代码与修复操作。须将 Restart 请求的 correlation ID 传入 R06 `stop_active`，嵌套停止不得另造 ID；`source:isos` 的 `deployment_stop_unconfirmed` 使用旧准入 run ID 和同一 correlation。仅当候选撤销不确定时，`source:isos` 的 `deployment_preflight_failed` 使用未发布候选的 manager-session 范围和同一 correlation。P07 current view 仍是旧 run，不能把 manager-session 清理事件改成候选 run 事件。响应及事件不得包含 grant 字节、原始子进程输出、私有路径或不可信探测文本。[撤销冻结用例](../../../tests/fixtures/skill_selection/retirement_cases.json)覆盖确认、unknown、双重故障、标记写入、关联和重试路径。本规则与 R06 的可检查停止原语组合，不重新定义 EOS 或 MuJoCo 的实体停止证明。

P01 损坏沿用 `package_changed`、`package_unsafe_path` 或 `package_missing_file`，指向条目 `/package`；P04 快照损坏沿用 `generated_manifest_mismatch`；P05 feature 缺失沿用 `reader_feature_unsupported`。R04 语义错误使用现有 schema-1 API 诊断 `{code,path,message,stage}`：`skill_selection_invalid`（`graph`）、`skill_package_incompatible`（`package`）、`skill_library_unsafe`（`package`）、`skill_library_changed`（`package`）。消息提供修复动作，不包含原始路径、库内容、grant 或 worker stderr。固定用例覆盖空 host、显式选择、重复、路径、符号链接、变更的链接包、旧 EOS feature、错误 generation、旧 PID 不变、环境隔离、无效 ABI、未声明能力及 worker 回收。完整输出保存于 `logs/test/<run-id>/`。

IsOS 选择切片 `df481774d80e8988cb8e17170fa071dfbf267672` 已通过独立所有者审查。与该精确版本、最终 IsOS 事件和候选预检版本及已审查的 EOS 版本组合的干净 home 跨所有者证据覆盖 P01 链接的原生技能：超大候选 catalog 在替换仍在服务的旧运行之前失败；探测后修改库也在替换边界被拒绝。见[带日期的证据与命令](../design/skill-selection_cn.md#验收状态2026-11-02)。这些测试使用临时组合；关闭 R04 前仍需依次更新并审查 PR，以及在最终合并版本上验收。
