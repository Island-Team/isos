# R06 依赖停止设计

[English](../../../docs/isos/design/dependent-stop.md) · [归属方契约](../interfaces/dependent-stop_cn.md) · [冻结用例](../../../tests/fixtures/dependent_stop/README_cn.md) · [首次使用](../usage/get-started_cn.md#r06-依赖停止目标)

**实现中，尚待验证。** [归属方契约](../interfaces/dependent-stop_cn.md)规定 API 字段、单位、终结原因和事件代码。本文说明管理器如何在 CLI、API、Ctrl+C、Restart、回滚、提供者丢失及 Shutdown 中作出同一停止判定。EOS 与 MuJoCo 仍各自拥有执行器和仿真器边界。

## 现有边界与改动

现有 `Supervisor::stop` 按逆启动顺序取出子进程，对每个直接子进程调用固定两秒 SIGTERM 宽限的 `terminate`，注销控制授权包，只返回进程 ID。`terminate` 不返回退出、进程组或实体停止结果。Restart 与提供者丢失路径也调用它而不携带终结结果。`Drop` 会再次调用，回收后才写停止事件。R06 用一个幂等停止操作替代分散调用，携带单调时钟期限、运行绑定结果和确定的逆依赖计划。信号对象仍必须是选中子进程句柄及记录的进程组；状态 PID 或图中的命令不能成为信号对象。

## 停止状态机

每个获准运行保持 `Running → Stopping → ConfirmedStopped | UnknownStopped`。Stop 原子关闭该部署新的管理器 Start/Restart 准入，快照选中子进程身份、依赖图、运行 ID 与授权，并在正常优雅停止信号前**持久写入未知停止意图**。若该有界写入失败或卡住，紧急信号继续执行，结果不可逆地为未知。并发 Stop 加入此状态，仅主操作发信号。发起调用方提供所有停止事件复用的关联 ID。就绪、自动重启及新授权不得与 Stop 竞争。`Drop` 只在未发布终结结果时执行有界尽力清理，不能把未知改成确认。意图写入后、发信号前或发信号期间崩溃均恢复为未知；不得用记录中的 PID 再发信号。

从**活动**的已准入进程规格构建逆拓扑顺序，不能假设某次崩溃后 `self.order` 仍完整。独立同层进程按逆启动下标排序。先停依赖者，再停提供者。EOS 提供者在所有其他已选图子进程退出或耗尽非 EOS 预算前保持运行。某依赖者忽略 TERM 时，在同一总期限内向记录的组发送 KILL 并继续，不得占用 EOS 预留窗口。提供者意外死亡时，同样按计划停止其依赖者，旧运行终结为未知；所有者证明安全 fence 前禁止自动恢复。每个子进程记录观测的退出状态、进程组排空、信号失败及归属方证明；按优先级选择一个固定原因，每个子进程恰好记录一次。

启动时持有直接子进程，记录组长 PID、会话及可选 pidfd。使用 Linux `waitid(..., WNOWAIT)` 观察退出但**不回收**，使组长 PID 仍被占用。最后一次计划的组信号前禁止 `Child::try_wait` 或 `wait`。每次 `kill(-pgid, SIGTERM|SIGKILL)` 前核对所持未回收组长与记录的 `/proc/<pid>/stat` 启动 ticks、PID／PGID 和会话；回收后或身份不确定时绝不向组发信号。pidfd 只能标识组长，不能在组长回收后授权负 PGID 信号。直接子进程退出 0 而同组后代仍活着，不代表组已排空。已验证组存活检查的 `ESRCH` 表示组空，`EPERM` 或成员歧义表示未知。现有状态／提供者轮询也必须改用同一非回收路径。若旧路径已回收组长，禁止向可能被复用的 PGID 发信号，并以未知失败关闭。最后一次安全信号后才回收直接子进程；后代不能证明消失时报告未知。即使强制 KILL 后成功回收，结果仍未知。有界进程组清理之后尝试注销授权，即使某个子进程仍未解决也如此；该运行已为未知，不能授权复用。注销失败或未及时完成须可见，且不得授权复用。

用同一单调时钟计算 `deadline = stop_start + stop_timeout_ms` 与 `cleanup_at = deadline - 2,000 ms`。优雅等待不得越过 `cleanup_at`；最迟到此时向剩余自有组安全发 KILL 并标记未知。非阻塞轮询直接子进程回收至多 1,500 ms，至少留 500 ms 给**实时**终结栅栏和响应。停止归属方与期限计时器在一个按运行 ID 绑定的内存状态格上串行。归属方持有栅栏时检查单调时钟、所有进程组及 EOS 证明，再原子选择终结值，并向共享状态和有界非阻塞响应队列发布同一值。确认发布必须严格早于期限；若队列不能接收，则发布未知。到达或超过期限时计时器发布未知，恰好等于期限也归未知。较晚的 OS 传输刷新不能改变已排队值。发布后所有调用方看到的状态格不可变；文件工作线程及日志收集器均不得修改。没有工作线程持久发布确认，因此信号前未知意图的重命名或父目录 fsync 即使在调用方返回未知后完成，也不会改变安全结果。管理器存活期间保留未回收句柄供后续有界清理；Shutdown 后绝不依据记录 PGID 再发信号。日志位于停止期限的关键路径之外。

EOS 专属确认步骤用专门的严格解析器在 P06 日志归一化之前读取选中子进程的原始 stdout 管道。唯一可接纳帧是至多 4096 字节、以换行结尾的 schema-1 `eos_terminal_stop` 对象。将 `run_id`、`eos_pid`、`active_revision`、规范 `backend_node`、所选 `parts` 及管道本身绑定到获准运行。重复／未知／缺失键、不完整／超长／第二条／过晚行以及错误管道帧，在进入有界收据通道之前拒绝。EOS 在 MuJoCo 和辅助进程中关闭继承的 stdout 写端；IsOS 要求唯一一行与 EOS 退出之后出现 EOF，但不会越过 EOS 宽限等待。依据[归属方契约](../interfaces/dependent-stop_cn.md)校验后端 PID／PGID 相等、generation／epoch 形状、四项停止事实、所有选中部件结果及固定原因码。EOS 独自证明嵌套 MuJoCo 组排空与双实体停止；IsOS 验证其结构化断言，并在实时栅栏确认前独立验证自己的 EOS 组排空、零码退出和组长回收。EOS 的 `managed-stop-v1` 能力 token 只是早期支持检查。其 15,000 ms 硬界限可容纳在缺省 20,000 ms IsOS 宽限内；配置更短的宽限或收据过晚均为未知。EOS 未知收据或非零退出即使其他事实看似正确，仍为未知。处理收据时先进入运行绑定的终结栅栏并检查总期限：期限已到且状态格未决定时发布 `stop_deadline_exceeded`。否则达到／超过 EOS 宽限的收据为 `eos_receipt_late`，之后才检查选中管道解析有效性。选中管道上的错误运行 ID 属无效；其他管道的帧被忽略，使选中管道仍为缺失。此前发布的终结原因不可改变。

## 生命周期调用处

显式 Stop 和附着运行的 Ctrl+C 共享结果。管理器 Shutdown 先停止所有部署，再停止独立服务和监听器；任一部署未知时保留持久未知结果，并在有进程退出码的场合返回非零。正常 Shutdown 不能为强制 KILL 的运行发出 `deployment_stopped`。Restart 先完成 P04/P05、软件包及候选授权预检，再停止旧运行。旧停止未知时不能启动候选或恢复的旧代，应返回类型化终结结果并注销候选授权。旧停止确认后，若候选启动失败，则用同一原语停止部分进程组，为候选运行发布 `deployment_failed`，只在新的运行 ID 和授权下恢复旧规格。若回滚停止未知，则不得恢复。提供者丢失时先按逆序停止依赖者，旧运行标记未知，在归属方证明安全 fence 前禁止自动恢复。这些规则避免通过进程重启重放不确定的运动。

现有 P06 与 P07 存储继续保留首条就绪／故障锚点和最近获准运行范围。P07 发布新运行后、首个子进程启动前，IsOS 为该运行原子创建**独立**的 `phase:running,outcome:null` 停止状态文件；旧运行文件不覆盖。两次发布之间崩溃或写入失败时，新运行视为未知并拒绝启动。正常优雅停止信号前为同一运行原子写入并 fsync `phase:stopping,outcome:unknown`；这个未知意图是 Stop 唯一的持久提交点。意图发布在总停止预算内至多占用 500 ms；失败或卡住时，过晚的线程只能将未知写入该运行文件，IsOS 同时发尽力紧急信号并在实时结果中记为未知。管理器重启后，最近运行的 `running`、`stopping`、缺失或无效文件都视为未知，因为旧子进程身份持有关系无法重建。此前实时确认的 Stop 也视为未知：没有持久确认记录，保留的 `deployment_stopped` 日志不能授权启动。实体证明齐备后，只有实时终结栅栏可报告确认；确认事件仅在该栅栏之后排队，日志写失败增加有界计数但不能改变终结值。错误只用固定原因，不含子进程输出或授权值。Stop 中的状态返回 `state:"stopping"`、当前运行及子进程快照；未知后即使没有子进程也返回 `state:"degraded"`。恢复会与 P07 最近运行身份核对部署／运行绑定，绝不从日志推断确认。

## 验证计划

单元测试使用假单调时钟、可注入的发信号／回收边界及 EOS 终结收据桩，覆盖逆依赖、确定的独立顺序、并发 Stop 合并、提供者意外死亡、TERM 宽限、在 `cleanup_at` 发 KILL、有界回收／实时终结、后代残留、组长提前回收与 PGID 复用竞争、旧运行收据、原始选中管道收据的形状与身份、JSON 重复／未知／缺失键、错误管道／第二条／过晚行、缺失 EOF、EOS 非零退出、较短 EOS 宽限、持久意图后崩溃、P07 发布与新运行重置之间崩溃、意图重命名或父目录 fsync 卡住、实时确认后管理器重启以及日志故障。边界测试启动真实进程组，包含忽略 TERM 的子进程、先于后代退出的进程、已选 EOS 与 MuJoCo 两个场景、GUI 不可用、仿真器崩溃及 socket 清理。Restart 测试注入候选失败及回滚失败，比较新旧运行 ID、授权和就绪事件。断言旧停止未知后候选没有启动，且未知结果不写作 `deployment_stopped`。

IsOS 独立设计评审须核对冻结的 EOS 收据／解析器边界及宽限算术。实现后运行 IsOS Cargo fmt、严格 Clippy、测试和构建、[用例检查器](../../../tests/fixtures/dependent_stop/check_cases.py)、干净 home 的 CLI/API/Ctrl+C/manager-shutdown 重放以及 EOS/MuJoCo R06 边界测试。完整输出存入忽略的 `logs/test/<run-id>/`，准确命令和结果写入本文，PR 合并前还须独立实现评审。桩测试通过不足以关闭 R06。

2026-11-02 设计校验基于 IsOS master `1db1aa52`：`/home/ashex/.cargo/bin/cargo fmt --all -- --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings` 和 `/home/ashex/.cargo/bin/cargo test` 均通过（191 个单元、6 个 CLI 指南、1 个已移除命令和 9 个连线准入测试）。`python3 tests/fixtures/dependent_stop/check_cases.py` 检查 22 个冻结场景；Python 检查器解析所有改动 Markdown 的本地链接和 JSON 示例；`git diff --check` 通过。完整 Cargo 和文档输出在忽略的 `logs/test/20261102-r06-dependent-stop-design/`。这些仅为设计检查，不是 R06 实现或实体停止验证。

2026-11-02 评审修订校验：`/home/ashex/.cargo/bin/cargo fmt --all -- --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings` 与 `/home/ashex/.cargo/bin/cargo test` 在未改动的 Rust 源码上通过。`python3 tests/fixtures/dependent_stop/check_cases.py` 检查 31 个用例，包括 Stop 前退出但未回收的组长、组长已回收／PGID 复用竞争、2,000 ms 清理窗口、持久停止意图后崩溃及新运行重置边界。相对 Markdown 链接、JSON 示例和 `git diff --check` 均通过。完整输出在忽略的 `logs/test/20261102-r06-dependent-stop-design-review-fixes/`。EOS 收据身份和宽限时间仍待归属方同意。

2026-11-02 终结栅栏修订校验：`/home/ashex/.cargo/bin/cargo fmt --all -- --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings` 与 `/home/ashex/.cargo/bin/cargo test` 在未改动的 Rust 源码上通过（191 个单元、6 个 CLI 指南、1 个已移除命令、9 个连线准入测试）。`python3 tests/fixtures/dependent_stop/check_cases.py` 检查 35 个设计用例，包括未知响应后才完成的意图重命名与父目录 fsync、期限相等以及实时确认后管理器重启。相对 Markdown 链接、JSON 示例、用例 JSON 和 `git diff --check` 均通过。完整输出在忽略的 `logs/test/20261102-r06-finalization-fence/`。EOS 收据身份、信号语义及宽限界限后来已在 EOS R06 归属方设计中获准；此校验不能验证实体停止。

2026-11-02 EOS 收据对齐校验：`/home/ashex/.cargo/bin/cargo fmt --all -- --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings` 和 `/home/ashex/.cargo/bin/cargo test` 在未改动的 Rust 源码上通过（191 个单元、6 个 CLI 指南、1 个已移除命令、9 个连线准入测试）。`python3 tests/fixtures/dependent_stop/check_cases.py` 检查 56 个设计用例，包括原始管道身份、无效／过晚／第二条收据、缺失 EOF、缺失支持 token、EOS 零码退出以及 15／20／30 秒期限关系。两个规范 JSON 用例与经评审的 EOS 归属方契约示例一致。相对 Markdown 链接、JSON 示例和 `git diff --check` 均通过。完整输出在忽略的 `logs/test/20261102-r06-eos-receipt-alignment/`。这是设计检查；仍须使用真实 EOS 和 MuJoCo 验证。

2026-11-02 原因优先级评审修订：`python3 tests/fixtures/dependent_stop/check_cases.py` 仍检查 56 个用例。选中 EOS 管道上的错误运行 ID 为无效；仅其他管道有收据时，选中管道仍为缺失。实时终结状态格在总期限到达或超过时若尚未决定，`stop_deadline_exceeded` 优先于过晚收据。总期限用例在强制清理之前已回收 EOS 组，不会由较早的强制 KILL 结果遮蔽该边界。相对链接、JSON 示例、`cargo fmt --all -- --check` 与 `git diff --check` 均通过。完整聚焦输出在忽略的 `logs/test/20261102-r06-reason-precedence/`。

## 2026-11-02 实现证据

主题分支现已为每次运行建立仅归属方可读写的 `running` 与 `stopping/unknown` 持久记录；运行范围内的终结单元由截止计时器保护。停止路径使用不回收的子进程首领观察，在进程组信号前核对启动时钟、会话与 PGID，并按依赖逆序停止。只有所选 EOS 原始 stdout 管道的严格收据可参与确认。Stop、Restart、启动失败、提供者丢失和管理器关闭共用监督器结果；未知结果阻止替换运行。重复 Stop 与状态返回同一实时终结结果；管理器重启后按 P07 最后运行 ID 保守恢复为未知。CLI 对未知停止返回非零状态。

可重放命令为 `/home/ashex/.cargo/bin/cargo fmt --all -- --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings`、`/home/ashex/.cargo/bin/cargo test`、`/home/ashex/.cargo/bin/cargo build`、`python3 tests/fixtures/dependent_stop/check_cases.py` 与 `git diff --check`。完整输出保存在忽略的 `logs/test/20261102-r06-implementation/`。单元和替身测试覆盖持久恢复、收据精确字段与重复键、所选原始管道的帧与 EOF、错误运行与管道写端滞留收据、首领已退出但未回收的真实 Linux 进程组、幂等归属方 Stop，以及在部分启动时列出尚未生成的所选子进程。用例检查器验证全部 56 个冻结设计用例的形状；它不逐个执行这些情景。

主 EOS debug 二进制尚未声明 `managed-stop-v1`，所以经由该可执行文件的托管启动在预检时拒绝。跨归属方重放必须使用 EOS R06 主题分支构建，且仍待其 generation-ID 收据修订。合并或声称 R06 合格前仍须完成真实 EOS/MuJoCo 双场景、实体停止证明、干净 home 的 CLI/HTTP/Ctrl+C 重放和独立实现评审。

独立实现评审发现两处会卡住 Stop 的问题。修订在实体清理前排队 `deployment_stopping`，并在实时终结栅栏决定后才向同一个有界日志工作线程交付终结结果。日志写入卡住不能阻塞管理器响应；队列拒绝时增加 P06 写入丢失计数。授权注销现在由工作线程执行，在最后 500 毫秒响应预留之前最多等待 500 毫秒。失败或超时将实时结果固定为 `control_grant_retire_failed`；过晚线程不能将其升级。聚焦测试使两个工作线程越过 Stop 响应仍保持阻塞，并检查终结值及事件顺序。最终格式、严格 Clippy、测试（200 个单元、6 个 CLI、1 个移除命令、9 个 edge）、构建、56 用例检查、Markdown 本地链接／JSON 及 diff 检查均通过。完整输出位于忽略的 `logs/test/20261102-r06-review-corrections/`。
