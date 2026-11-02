# R04 技能选择事务

[English](../../../docs/isos/design/skill-selection.md) · [所有者契约](../interfaces/skill-selection_cn.md) · [生成配置](../interfaces/configuration_cn.md) · [软件包验证](../interfaces/package-verification_cn.md)

**IsOS 选择切片已通过独立审查；R04 最终合并版本的门槛仍待完成。** R04 扩展已实现的 P01/P04/P05 与 R03 边界。图在 EOS component 上选择有界的精确原生库；IsOS 不解释 ABI 描述符，也不执行技能代码。EOS host ABI 与 worker 契约负责这些行为。这里不新增图技能节点、步骤执行器、ActionList 或第二条执行器路径。

Apply 首先验证保存的 `0.1.0` 图与精确 EOS component 位置；按拒绝未知字段的结构解析最多 16 个 `skill_libraries` 项，在精确数组索引 JSON Pointer 上拒绝重复元组、错误 pin 和不安全相对路径。旧图读取器不投影此设置。然后将每项加入 P01 图 pin 集合；每个精确包重新验证一次，要求 `kind:"skill"`，以 nofollow 检查库路径分量、普通文件、1–64 MiB 上限及 SHA-256。选中库、收据、发布元数据和每个普通 payload 文件均须由有效 UID 拥有且 group/other 不可写（`mode & 0o022 == 0`），与 EOS worker 前检查一致。仅使用已验证句柄，不扫描 store 或递归搜索 `*.skill.so` 来选择库。同包可显式选择多库。

若列表非空，探测选中 EOS runtime 的 `host-skill-abi-v2`。从已验证句柄按顺序生成 `isos/skill-selection.json`，把摘要加入 P04 的 `files`，把收据加入 `packages`，并使用原 P04 规范 manifest 算法计算 generation。发布前重检文件身份和摘要。包或二进制变更需要重新 Apply，即使图字节未变。发布沿用 P04 的 fsync、selection-map、marker 与图 rename 次序；不确定的 rename 只能通过重新读取已观察到的图 revision、map、marker 和完整 leaf 来判断。

IsOS 的 G06 聚合 catalog 发布由 IsOS 拥有的精确 `isos.process.component@v0.1` 描述符，包含 `env_from` 与结构化 `skill_libraries`；EOS 只提供通用 `SettingType::ObjectArray` 和有界 schema 支持。旧 EOS fallback clone 仅能让无技能选择的图使用 `env_from`，不能声明或准入非空选择；历史 `@v1.1` 描述符不变。

Run/Restart 在 deployment lock 下执行现有 G01/G06/P01/P04/P05 与 R03 预检，要求非空选择必须指向明确的 schema-2 V2 leaf，重新计算图、文件、软件包与库摘要，将新渲染的选择字节和 manifest 文件逐字节比较，再检查全部待启动进程并解析 secret。只有预检通过才分配新的 R03 run，并只向 EOS 子进程传 `EOS_SKILL_SELECTION_FILE`。config、bridge、EGIA、MuJoCo 与独立 service 均不得接收。预检失败保留旧 PID 和 grant。自动 EOS 重启沿用同一文件；显式 Restart 再检查并轮换 R03 run/grant。失败的候选运行按现有恢复路径处理，并使用新 run ID。

EOS 在目录变成可用前再次检查选中 manifest 与库内容。链接 payload 可能在 IsOS 预检与 EOS 启动之间变化；EOS 以 nofollow 打开同一文件句柄，哈希并复制到私有暂存，受限 worker 只加载该副本。Catalog 准入不能调用 handler；无效 ABI、描述符或不支持的已声明 callback 类别在调用前失败。若有效 handler 随后请求未声明的 primitive，Broker 在 handler 已启动后、Dispatcher release 前拒绝该请求。此负面边界不宣称能隔离任意同 UID 或内核受损的软件包代码。

## 候选预检事务（目标设计）

实现候选（2026-11-02）：`restart_processes_with_plan` 和首次 Run 现在创建尚未发布的候选 grant，调用所选 EOS 私有目录预检，解析封闭结果，并在发布 run scope 或停止旧子进程前复核选中字节及 grant。可检查的撤销路径和持久清理标记阻止清理不确定后的替换。实际启动时再次检查所选 EOS 程序、profile、选中文件、generation 和进程规格；EOS 独立重复准入。stub 进程测试覆盖清理环境、有效封闭拒绝、期限和进程组回收。18 个冻结用例仍是资格表；真实 EOS 双代重放和独立实现复核尚待完成。

验证命令：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --no-fail-fast`、`cargo build --all-targets`、`python3 tests/fixtures/skill_selection/preflight_cases.py`、`python3 tests/fixtures/skill_selection/check_cases.py`。通过的运行包含 213 个 Rust 单元测试、七个 CLI 指南测试、一个移除命令测试、九个 edge 测试及两个用例检查。完整输出在忽略的 `logs/test/20261102-r04-isos-candidate-preflight-impl/`。

独立复核修复证据（2026-11-02）：私有探测现在只传所选 EOS 子进程的有效 `ISOS_GPU` 和 `ISOS_ROS` asset 选择器，遵循图中环境、source 移除及已解析 `env_from` 的优先级。IsOS 在停止旧子进程前与 EOS 实际启动时再次检查相同值。主清理标记写入失败时，当前 manager 以内存状态阻止替换，并在 data root 写入备用标记；探测清理不确定时撤销候选 grant 文件，但保留空 run 目录作为持久遗留标记；即使两个标记写入均失败，manager 重启后仍阻止替换。故障注入覆盖标记写入失败。针对 EOS 提交 `b5bf113adf7760efc8f98114d0d5aaa090fac62b` 与双 asset 原生技能发布的真实重放接受匹配的 `nvidia`/`jazzy` 组合，并对缺失选择器或错误 ROS 返回封闭的 `package/package_invalid`。这证明 asset 选择器一致性，尚非完整双代部署重放。复核修复检查通过 215 个 Rust 单元测试、七个 CLI 指南测试、一个移除命令测试、九个 edge 测试、格式检查、严格 Clippy、全 target 构建及两个用例检查。完整命令与输出在忽略的 `logs/test/20261102-r04-isos-candidate-preflight-review-fix/`。

`restart_processes_with_plan` 现在于准备 R03 grant 后、调用 `managers.remove(name)` 或 `active.supervisor.stop()` 前探测候选。该事务仅对非空选中 host 增加[私有候选预检](../interfaces/skill-selection_cn.md#候选-abi-预检isos-实现候选)。`preflight_packages` 仍负责图、P01/P04/P05 与选中进程规格。候选 `Supervisor` 拥有新 run ID 与 grant；探测期间不发布 run scope；探测自身不修改旧 manager 表项、子进程或日志范围。

1. 在 deployment lock 下固定保存图字节和修订、选中 schema-2 leaf、manifest/generation 完整清单、精确选中 `aether-runtime` 文件及 feature、选中字节、P01 收据/发布/payload/库摘要、实际 `--node-types` 参数、secret 和进程规格。先按现状验证旧运行 grant，再准备候选 grant。`host-skill-abi-v2` feature 声明本身不是 ABI 准入。
2. 用精确程序、EOS 私有预检参数、清理后的 manager 环境、候选 verifier/选中文件以及经验证的有效 `ISOS_GPU`/`ISOS_ROS` asset 选择器启动探测。对进程组实施统一 90 秒墙钟期限、有界 stdout/stderr 收集和已确认回收。只解析一行封闭 JSON，逐项比对回显及有序库摘要。有效拒绝、无效输出、超时和清理不确定映射为四个固定 IsOS 代码；日志不出现原始输出或 guest 文本。
3. 探测失败或随后停止前重检失败时，以新增的可返回错误并证明目录删除的显式 API 撤销候选 grant。`Supervisor::stop()`/Drop 只打印错误，不能作为证明。确认探测进程组清理；任一清理无法确认时优先报告 `skill_catalog_preflight_cleanup_unconfirmed`。在 `managers.remove(name)` 前返回，保留旧 PID、run ID、修订、grant 和 run scope；无需恢复 run。候选目录修复前禁止下一次替换。
4. `ok:true` 后立即重新哈希保存图、选中 generation 完整清单、程序字节及 P01 包/库输入，验证候选 grant 与进程规格。这是最后的停止前门槛，候选 grant 继续用于启动。旧 `ActiveDeployment` 与 run 身份仍可寻址时调用 R06 可检查停止；只有包含 EOS 实体停止、进程组清空和旧 grant 撤销的实时 `confirmed` 结果，才允许发布候选 run scope 并启动替换进程。新 EOS 用新暂存字节独立重复选中文件、grant、包、ABI 与纯目录就绪准入；不缓存探测结果。旧运行确认停止后的启动失败才走现有恢复路径。同一 run 的 EOS 自动重启重复启动准入，无须替换预检。

### 旧运行停止不确定的事务边界

已合并的 R04 路径在 `managers.remove(name)` 后调用无结果的 `Supervisor::stop()`。它只返回进程 ID 并打印 grant 撤销错误，无法证明 EOS terminal receipt、实体停止或 grant 状态，因此不能作为成功探测后的替换提交授权。IsOS R06 [#57](https://github.com/Island-Team/isos/pull/57) 引入可检查的旧运行停止 terminal，但 Restart 遇到 `unknown` 时仍把未发布候选 bundle 留给隐式清理；本 R04 切片须实现显式撤销，不能把它当成已验收的 R06 行为。专门实现须保留 manager 表中的旧部署，直到 R06 停止结果固定，再按其不可变 terminal 分支。仅在 `confirmed` 后发布候选 P07 范围。若为 `unknown`，保留旧表项、原 run ID、活动修订和 terminal-degraded 状态；不得启动候选或旧规格恢复，不得记录候选 `deployment_starting`，不能把普通 Drop 当作证明。保存图与选中 generation 不变；Stop 期间旧 PID 集合可能变化。

unknown 时按顺序：关闭替换准入，保留旧 R06 unknown stop intent 与 terminal 原因，尝试可检查的未发布候选 grant 撤销，确认没有探测子进程，再返回结构化 Restart 故障。旧 bundle 由 R06 停止结果管理，不能悄然脱离或宣称已撤销。候选 bundle 从未交给托管子进程。候选撤销成功时返回旧运行 `deployment_stop_unconfirmed` 故障；若候选撤销失败或不可证明，则 Restart 错误以 `skill_catalog_preflight_cleanup_unconfirmed` 优先，同时旧 terminal 和停止事件保留实时 R06 的精确原因，如 `eos_stop_unconfirmed`、`stop_group_unreaped` 或 `control_grant_retire_failed`。把现有 Restart 请求 correlation ID 传给 `stop_active` 及其排队的 R06 terminal 事件，嵌套停止不得另造 ID。旧运行 `source:isos` 的 `deployment_stop_unconfirmed` 使用该 ID 和旧准入 run 范围。若候选清理也失败，其 `source:isos` 的 `deployment_preflight_failed` 使用同一 correlation，但属于 manager-session 范围，因为候选 run 尚未发布。不得为 IsOS 停止／清理故障伪造 EOS `stage/code` 或 P01 `package`。

沿用候选清理阻断和持久标记写入，包括 data root 备用标记与遗留目录检查；旧运行 unknown terminal 是独立准入阻断，标记缺失不能证明安全。候选撤销不能证明时，返回前持久化标记；两个标记写入均失败时，当前管理器保留内存阻断，遗留候选目录在重启后阻断。标记及目录需操作员检查候选清理和 R06 旧运行恢复 fence 后才能移除。同一管理器内重试保留原 R06 原因并在无新候选分配的情况下拒绝。管理器重启后，旧 P07 run ID 不变；有效停止记录恢复为 `stop_interrupted`，缺失／无效记录恢复为 `stop_result_persist_failed`，不得从日志重建 `eos_stop_unconfirmed` 或其他早先原因。两种恢复后的 unknown 状态都拒绝 Start/Restart。确认停止后的候选启动故障可用新 run 恢复；旧 Stop 不确定绝不可恢复启动。

故障注入须发生在私有 `ok:true` 和最终字节／grant 复核之后，不能只测试探测拒绝。覆盖：（1）旧运行确认停止且撤销成功，只发布一次候选；（2）EOS unknown receipt 或进程组未证实清空，候选 grant 删除且无候选发布；（3）旧 grant 撤销失败，旧 terminal reason 为 `control_grant_retire_failed`；（4）旧 Stop unknown 且候选撤销失败，两个原因均保留且清理诊断优先；（5）主、备用标记写入失败但保留遗留目录；（6）同一管理器内重复 Restart/Start 与管理器重启后的恢复均不启动。断言没有虚假的 `deployment_ready` 或 `deployment_stopped`，旧 P07 run 身份及保存字节稳定、状态 degraded、旧运行仅一条 `deployment_stop_unconfirmed`、精确安全 code/path、旧运行停止与 manager-session 清理事件使用同一个 Restart correlation、日志无候选 bearer，以及 unknown 后无子进程启动。同一管理器重试保留早先原因；管理器恢复则使用上述 `stop_interrupted` 或 `stop_result_persist_failed`。[冻结用例](../../../tests/fixtures/skill_selection/retirement_cases.json)固定这些边界。与 R06 独立实现合成后，须重新变基并跨所有者重放，才能声称集成停止已验证。

实现证据（2026-11-02）：本分支将获批事务变基到已审查的 R06 版本 `3567dad`。旧 manager 表项在 `stop_active` 期间仍可寻址；terminal 为 unknown 时可检查地撤销未发布候选，不进入范围发布或恢复启动。候选清理失败写主标记或 data root 备用标记；两者都写失败时保留遗留目录。选中技能的 Restart 将请求 correlation 传给旧停止事件；普通外部 Stop 仍使用 R06 自己的 correlation 行为。六个探测后单元用例覆盖确认及 unknown 停止、缺少 EOS receipt、旧 grant 撤销失败、候选清理失败、标记备用、重试、恢复及事件范围。`cargo fmt --check`、严格 Clippy、`cargo test`（237 个单元、七个 CLI 指南、一个移除命令和九个 edge 测试）、`cargo build --all-targets`、20 个选择用例、18 个预检用例与 dependent-stop 用例、reader 文档检查及 `git diff --check` 均通过。完整输出位于忽略的 `logs/test/20261102-r04-checked-retirement/`。独立实现复核及最终真实 EOS/MuJoCo 双代重放仍未完成。

独立复核修复（2026-11-02）：API Start 和 Restart 现在先检查候选清理标记、遗留 grant 目录与旧运行 unknown terminal，再读取或验证保存图、包输入和生成的 core 设置。候选清理不确定优先于旧停止不确定。grant 根目录枚举及条目／文件类型检查失败均映射为固定的 `skill_catalog_preflight_cleanup_unconfirmed` 诊断和空 path；响应不含私有路径或原始 I/O 文本。完整客户端路径回归用例在旧停止 unknown 和双重清理失败后删除或修改保存图字节，再重试 Start/Restart，确认无新候选、范围发布或子进程。另有注入式检查覆盖根目录、条目和文件类型错误。基于已合并的 IsOS R06 master `19a251e`，`cargo fmt --check`、严格 Clippy、`cargo test`（240 个单元、七个 CLI 指南、一个移除命令和九个 edge 测试）、`cargo build --all-targets`、20 个选择、18 个探测／六个退休、57 个 dependent-stop 与双语 reader 文档检查，以及 `git diff --check` 均通过。完整输出在忽略的 `logs/test/20261102-r04-retirement-review-fix/`。最终独立复核和真实 EOS/MuJoCo 重放仍未完成。

P01 拒绝发布版本 `1`，因此它不能用于验证选中 pin 的跨所有者边界。P01 可以验证版本超过 EOS 64 字节选中 pin 上限的发布；IsOS 现在于精确的图 `/package` 指针以 `skill_selection_invalid` 拒绝该选择，发生在 Apply 发布及候选 grant/探测分配之前。IsOS 在此图检查中实施 EOS 的 96 字节包 ID 与 64 字节版本上限。封闭的 EOS `selection/selection_invalid` 响应仍是错误交接的协议解析用例，不能证明普通 P01 有效的选中发布会产生这一探测响应。合成探测测试绕过包链接和验证，只有使用真实包的测试证明所有者门槛。

选中 pin 审计证据（2026-11-02）：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --no-fail-fast`、两个技能选择用例脚本与 `git diff --check` 均通过。完整测试包含 216 个 Rust 单元测试、七个 CLI 指南测试、一个移除命令测试和九个 edge 测试。新增的真实包单元测试链接并验证 P01 有效的超长 pin，然后检查精确图拒绝。完整输出在忽略的 `logs/test/20261102-r04-ipf07-pin-audit/`。

探测可在旧子进程停止前拒绝无效 ABI、不支持的 callback、缺失静态 primitive binding、重复身份与超大目录；不能证明实时 adapter 健康、Dispatcher 状态、R05 实体 release fence 或 R06 actuator 停止与模拟器回收。探测绝不调用 guest handler；有效 handler 后续发出的未声明调用由 EOS Broker 在 Dispatcher release 前拒绝，维持唯一执行器准入路径。

验证使用[机器用例](../../../tests/fixtures/skill_selection/README_cn.md)。单元测试检查指针、pin 收集、排序与去重、规范字节、P01 链接变化、P04 generation 和子进程环境。stub 集成测试启动两代进程：篡改库和旧 EOS feature 在停止第一代前拒绝；有效选择只到达 EOS。EOS owner replay 检查错误 revision、ABI/描述符、未声明能力和 worker 回收。完整命令与输出放入 `logs/test/<run-id>/`；实现后在此记录结果。

候选预检设计证据（2026-11-02）：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、两个用例检查、变更文件相对链接扫描与 `git diff --check` 均通过。Cargo test 报告 207 个单元、七个 CLI 指南、一个移除命令和九个 edge 测试。完整本地输出在忽略的 `logs/test/20261102-r04-isos-candidate-preflight-design/`。固定 EOS 设计提交：`cf48e439c88d7938f5d5e9cf839b066883359d36`。这些检查仅证明文档与用例一致；实现、跨仓重放与独立设计复核仍未完成。

候选预检设计用例：`python3 tests/fixtures/skill_selection/preflight_cases.py` 检查 18 个所有者用例和 EOS 封闭结果示例。这仅是契约形状证据；进程测试、跨所有者重放与独立设计复核仍未完成。

设计用例检查（2026-11-02）：`python3 tests/fixtures/skill_selection/check_cases.py` 通过 20 个用例 ID 与选择文件结构；`git diff --check` 通过。变更文档的相对链接扫描未发现缺失目标。完整本地输出在忽略的 `logs/test/20261102-r04-isos-design/`。这只检查契约结构；实现与跨所有者重放仍未完成。

IsOS 实现候选（2026-11-02）：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`、`cargo test skill_selection -- --nocapture` 与 `python3 tests/fixtures/skill_selection/check_cases.py` 均通过。完整输出位于忽略的 `logs/test/20261102-r04-isos-impl/`。专项测试覆盖精确图指针、重复与不安全路径、已验证 P01 pin、链接 payload 修改、不兼容发布类型、私有 V2 generation 清单及篡改拒绝，以及只向 EOS 子进程交付环境变量。20 用例检查仅验证冻结表格与 schema；EOS 拥有的 I17–I20 和跨所有者双代运行重放仍需集成证据，才能关闭 R04。

独立审查修复（2026-11-02）：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --no-fail-fast`、`cargo build --all-targets`、`cargo test skill_selection -- --nocapture`、`python3 tests/fixtures/skill_selection/check_cases.py` 与 `git diff --check` 均通过。完整输出位于忽略的 `logs/test/20261102-r04-isos-review-fix/`（204 个单元测试、七个 CLI 指南测试、一个移除命令测试、九个边检查测试；12 个专项 skill-selection 单元测试及一个 CLI 测试；20 个冻结用例）。回归测试覆盖 FIFO 替换时不阻塞、无符号链接且有界的选择文件读取、丢失 EOS 交付、setup/capabilities 辅助进程环境隔离、旧版 HTTP Validate 优先返回字段指针，以及 EOS 64 KiB 选择文件限制。跨所有者资格验证仍待完成。

跨所有者所有权检查对齐（2026-11-02）：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --no-fail-fast`、`cargo build --all-targets`、`cargo test skill_selection -- --nocapture`、`cargo test writable_selected_skill_rejects_restart_before_old_process_stops -- --nocapture`、`python3 tests/fixtures/skill_selection/check_cases.py` 与 `git diff --check` 均通过。完整输出位于忽略的 `logs/test/20261102-r04-isos-owner-mode/`（207 个单元测试、七个 CLI 指南测试、一个移除命令测试、九个边检查测试；14 个专项 skill-selection 单元测试及一个 CLI 测试；20 个冻结用例）。测试拒绝选中库与无关 payload 文件的 `0775` 模式、拒绝 group 可写的 P01 收据和发布元数据，并证明失败的 Restart 保留旧 PID 与 run ID。跨所有者资格验证仍待完成。

## 验收状态（2026-11-02）

IsOS 选择所有者版本 `df481774d80e8988cb8e17170fa071dfbf267672` 已通过独立审查。干净 home 重放将该精确版本与 IsOS 事件版本 `7414f04b3e824ed29eb70cd799a9a2be4e94a546`、候选预检版本 `6e2bb328c61585e271e2f7c51f253c535e03bcc7` 和已审查的 EOS foundation、service、私有预检版本组合。P01 链接的原生 C guest 使候选 Restart 在停止旧子进程前返回 HTTP 422 `skill_catalog_preflight_rejected`，旧的类型化 SkillService 仍可使用，run ID、revision 与三个 PID 均不变；候选授权目录撤销后，旧授权目录仍是唯一的授权目录。JSONL 轮转后仍可查到管理器会话故障与旧的就绪锚点。完整输入、命令与输出见 EOS 忽略目录 `logs/test/20261102-r04-oversize-cross-owner/qualification.md`。

另一组干净 home TOCTOU 重放在私有 `ok:true` 之后修改链接库字节：旧进程停止前，Restart 返回 `package_changed` 且保留旧运行；旧进程停止后，候选启动失败，恢复原字节后旧 revision 在新运行中恢复。IsOS 交接之后的自动 EOS 重启遇到修改字节时，在就绪前被 EOS 新一轮准入拒绝。精确 hook、命令、结果与局限见 EOS 忽略目录 `logs/test/20261102-r04-toctou-final-heads/qualification.md`。组合后的 IsOS tree 通过 `cargo fmt --all -- --check`、严格 Clippy、`cargo test --no-fail-fast`（220 个单元、七个 CLI、一个移除命令和九个边检查测试）、`cargo build --all-targets`，以及 20 个选择、18 个预检和 31 个事件/10 个游标 fixture 检查；命令与完整输出见 EOS 忽略目录 `logs/test/20261102-r04-sequential-review/qualification.md`。这些均为临时组合；每次前置 squash 后仍须更新并独立审查依赖 PR，在最终合并版本上重放后才能关闭 R04。

## 精确诊断路径

Validate、Apply、Run 和 Restart 必须在通用 `ProcessSettings` 反序列化之前对原始图运行 R04 预验证器；现有通用解析器会把嵌套错误收缩为节点路径，丢失字段或数组索引。只有位于 `isos/process/component/eos`、解析到 `aether-runtime` 的 `@v0.1` 节点可以选择库；`bridge`、`config` 和其他 EOS-role 节点即使 `role` 同为 `eos` 仍须在各自 `/settings/skill_libraries` 指针失败。

前置合并版本更新（2026-11-02）：候选预检已变基到 IsOS `master` `c716b20942f294a6f59e58a19af596cd1c8b1e4b`，包含通过审查的选择与受控子进程事件。组合版本通过 `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --no-fail-fast`（220 个单元、七个 CLI、一个移除命令和九个边检查测试）、`cargo build --all-targets`、20 个选择、18 个预检、31 个子进程事件、37 个 JSONL、22 个 wire／12 个状态游标用例，以及 `git diff --check`。完整输出和命令位于忽略的 `logs/test/20261102-r04-candidate-merge-refresh/`。关闭 R04 前仍需独立复核本分支并在最终合并版本上进行跨所有者重放。
