# R06 部署依赖停止契约

[English](../../../docs/isos/interfaces/dependent-stop.md) · [设计](../design/dependent-stop_cn.md) · [首次使用](../usage/get-started_cn.md#r06-依赖停止目标) · [冻结用例](../../../tests/fixtures/dependent_stop/README_cn.md) · [R06 任务](../../../../Island-eos/docs/island/implementation-todo.md)

**目标设计，尚未实现。** IsOS 管理其直接子进程 EOS、EGIA、bridge 和 config 的进程组、同用户管理器及 loopback API 的响应、以及持久的部署终结状态。EOS 管理 Dispatcher 停止准入、底盘和机械臂实体停止及其直接 MuJoCo 子进程；MuJoCo 负责仿真器确认。IsOS 不发送执行器命令、不接管仿真器，也不把 PID 消失解释为安全停止。

## 调用方、边界与结果

同 UID 的 CLI 使用现有 schema-1 管理器 socket 的 `deployment_stop`；工作台使用 `POST /api/isos/v1/process-control` 和 `operation:"stop"`。附着的 `isos run` Ctrl+C、Restart、启动失败回滚、上游进程丢失后的恢复以及 `manager_shutdown` 使用同一个监管器停止原语。HTTP 请求的可选 `revision` 保持原有的旧编辑视图拒绝语义。图、软件包和 API 设置都不能延长 R06 停止期限。不增加保存图字段，也不改变 schema-1 外层响应。

```json
{"schema_version":1,"operation":"stop","revision":"7a8daee6b7db9105f5483506ffc1743625b63089710dfbf936622329c471016b"}
```

停止请求返回原有按逆依赖顺序排列的 `stopped` 数组，并新增 `terminal` 对象。`stopped` 只表示 IsOS 的直接子进程已被回收，不证明实体停止。重复 Stop 返回空 `stopped` 和最近获准运行保留的终结结果。尚未运行过的 home 返回 `terminal:null`。新 Run/Restart 获准后，该新运行在开始 Stop 前使用 `terminal:null`；替换前的预检拒绝保留旧运行及结果。

```json
{"schema_version":1,"ok":true,"data":{"stopped":["egia","bridge","config","eos"],"terminal":{"run_id":"r42","outcome":"confirmed","reason_code":null,"processes":[{"id":"egia","outcome":"confirmed","reason_code":null},{"id":"bridge","outcome":"confirmed","reason_code":null},{"id":"config","outcome":"confirmed","reason_code":null},{"id":"eos","outcome":"confirmed","reason_code":null}]}},"error":null}
```

`outcome:"confirmed"` 要求所有选中的直接子进程组在期限内退出并被回收，且 EOS 的 R06 归属方停止证明成功。任何证明缺失、超时、信号失败、子进程意外退出、强制 KILL、EOS 实体停止未确认、进程组后代残留或授权注销失败均为 `unknown`；进程消失不能将其升级为确认。`reason_code` 仅在确认时为 null，否则取 `child_exit_unexpected`、`stop_signal_failed`、`stop_deadline_exceeded`、`stop_forced_kill`、`stop_group_unreaped`、`eos_stop_unconfirmed`、`eos_receipt_missing`、`control_grant_retire_failed`、`stop_result_persist_failed` 或 `stop_interrupted`。任一子进程结果未知，整体结果即未知。部分完成时仍为每个选中子进程提供结果。未知停止是已完成的管理器请求，`ok:true` 但 CLI 以非零状态退出；调用方必须检查 `terminal.outcome`，不能以 HTTP 状态判断安全。传输及归属方认证错误继续使用原有 `ok:false` 外层。

| 字段 | 类型／单位 | 用途、缺省、校验及兼容性 |
| --- | --- | --- |
| `terminal` | 对象或 null／— | process-control、管理器回复及 `GET /process-status` 的新增结果。从未获准运行或新获准运行尚未开始 Stop 时为 null；Stop、失败和管理器重启后保留。损坏或无法读取的记录报告 `unknown` 与 `stop_result_persist_failed`，不重建为确认。 |
| `terminal.run_id` | ASCII 部署运行 ID／— | P07 的最近获准 `r...` ID，1–64 位字母、数字、`_` 或 `-`；不能是管理器会话 ID 或调用方传入 ID。 |
| `terminal.outcome` | `confirmed` 或 `unknown`／— | 所有子进程停止证明；无隐式缺省。旧客户端可忽略新增对象，但不能把 `stopped` 当作实体安全证明。 |
| `terminal.reason_code` | 上述代码或 null／— | 停止顺序中第一项安全相关故障；确认时且仅确认时为 null。不包含原始 OS 错误或路径。 |
| `terminal.processes` | 对象数组／条 | 每个选中直接子进程一条，按逆依赖顺序，最大为获准进程数。MuJoCo 归 EOS 所有，不列为 IsOS 子进程。已提前退出的子进程除非此前已有归属方证明，否则为未知。 |
| `terminal.processes[].id` | 注册进程 ID／— | 精确的 IsOS 进程 ID，1–48 位 ASCII 字母、数字、`_` 或 `-`，不是 PID。 |
| `terminal.processes[].outcome` | `confirmed` 或 `unknown`／— | 直接子进程组及归属方证明；无缺省。 |
| `terminal.processes[].reason_code` | 上述代码或 null／— | 固定原因；确认时且仅确认时为 null。 |

IsOS 在 `0700` 私有目录下以 `0600` 普通文件保存一份 `data/stops/<deployment>.json` 状态记录，不跟随符号链接。字段为 `schema_version:1`（无符号整数）、`deployment`（已验证的部署 slug，1–48 位 ASCII 字母、数字、`_` 或 `-`）、`run_id`（精确 P07 部署运行 ID）、`phase`（`running`、`stopping` 或 `terminal`）、`outcome`（运行中为 null、停止中为 `unknown`、终结时为 `confirmed` 或 `unknown`）及 `reason_code`（null 或列出的固定原因）。`selected_processes` 是按计划逆依赖顺序排列的有界、唯一注册进程 ID 数组，在首个子进程启动前写入，并保留至停止阶段，以便崩溃恢复报告每个选中子进程。有界的终结结果 `processes` 数组只在 `phase:terminal` 出现。未知键、缺失字段、运行／部署不匹配、不安全元数据或非法转换不能解释为确认。IsOS 原子替换整份文件并 fsync 文件及父目录，不在原位更新另一运行。P07 持久的最近运行身份仍负责关联；此文件不是新的图或日志流。

发出任何停止信号前，IsOS 为同一运行持久地把 `phase:running` 替换为 `phase:stopping,outcome:unknown`。管理器在这一转换期间崩溃或写入结果不确定时，恢复为未知。管理器重启时，最近运行的 `phase:running` 记录同样视为未知，因为 IsOS 不能重新持有旧直接子进程的监管身份；只有已持久确认的终结记录仍可确认。新运行在 P07 发布其 ID 后、首个子进程启动前，原子重置为新 `run_id,phase:running,outcome:null`；旧运行未知时，未经归属方评审的安全恢复不得替换它。若两个写入之间崩溃造成 P07 与停止状态不一致，新运行按未知处理并阻止启动。意图写入失败也不能让硬件继续运动而无人处理：IsOS 仍发出有界紧急停止信号，将内存结果记为未知，并以 P07 持久运行 ID 加上任何非终结停止状态在重启时恢复未知；不能报告确认。确认回复要求 `phase:terminal,outcome:confirmed` 原子写入并 fsync；失败或过晚的持久化保留原先的未知意图，并报告 `stop_result_persist_failed`。恢复尚未完成的 `phase:stopping` 时报告 `stop_interrupted`。

## 停止期限及 EOS 证明

拟议的 IsOS 预算使用单调时钟 `deployment.stop_timeout_ms` 总期限，类型为无符号整数毫秒，缺省 **30,000**、有效范围 **5,000..120,000**，只能在 `config/isos.toml` 设置。`deployment.eos_stop_grace_ms` 也是无符号整数毫秒，缺省 **20,000**，有效范围从 **1,000** 至 `stop_timeout_ms - 3,000`，在依赖者停止后为 EOS 预留时间。非 EOS 进程组共享至多 `min(8,000, stop_timeout_ms - eos_stop_grace_ms - 2,000)` 毫秒的 TERM 宽限。关闭停止准入时开始总期限；`cleanup_at = deadline - 2,000 ms`。**最迟在 `cleanup_at`** 结束优雅等待，对仍存活且身份可安全确认的自有进程组发送 SIGKILL，将强制目标记为未知，并开始非阻塞回收轮询。最后窗口至多 1,500 ms 用于 KILL／回收，至少 500 ms 用于终结状态发布。届时仍未回收的子进程为未知，并留给后续尽力清理，不再向身份未固定的组发信号。终结状态工作线程可准备并 fsync 临时文件，但只有在停止状态锁下检查实时期限后才能重命名为确认；fsync 过晚则丢弃临时文件，保留持久的未知意图。调用方不因工作线程、收集器或子进程超过期限等待。字段缺失使用缺省值；格式错误、溢出或无效组合在管理器配置校验及新运行前以 `stop_budget_invalid`、`stage:"config"`、`path:"/deployment/stop_timeout_ms"` 或 `"/deployment/eos_stop_grace_ms"` 拒绝。每次获准运行保留经过校验的预算，之后配置损坏不能阻止紧急 Stop。这些是归属方设置，不是图字段。**EOS 优雅停止界限和预留公式仍待 EOS 归属方同意。**

只有 IsOS 保留尚未回收的已启动直接子进程**组长**时，向进程组发信号才有安全身份依据。Linux `waitid(..., WNOWAIT)` 可观察退出而不释放其 PID；就绪与提供者丢失轮询在最后一次组信号之前不能使用会回收的 `try_wait`。尚未回收的组长保留 PID／PGID，阻止负 PGID 被复用。IsOS 在启动时及每次 `kill(-pgid, signal)` 前核对所持组长的 `/proc/<pid>/stat` 启动 ticks、进程组和会话；无法读取或不匹配均属歧义。不再可能发组信号后才回收组长。pidfd 可识别组长，但在组长被回收后并不能固定 PGID。`EPERM`、身份意外变化、组长已被回收或组成员不确定时，以 `stop_group_unreaped` 失败关闭：不得向可能被复用的 PGID 发信号，也不得报告确认。管理器崩溃后，保存的 PID／PGID 不能用于发信号，因为未回收组长的持有关系已经消失；恢复结果为未知。直接子进程退出并不证明后代已离开组。

EOS R06 需要一个绑定精确运行 ID 和观测 EOS PID 的**机器可检查的归属方终结结果**，证明 Dispatcher 关闭准入、所需底盘及机械臂停止得到确认或明确 fence、MuJoCo 子进程被回收、归属方 socket 已清理。进程退出码或一行普通 `eos_stopped` 本身不足以证明双实体确认。拟议传输为选中子进程管道接纳的有界结构化 EOS 终结收据，同时交叉检查退出状态和子进程身份；精确事件代码、字段、信号语义和 EOS 最大界限**仍待 EOS 归属方评审**。在该契约被评审通过前，IsOS 必须把 EOS 归为 `unknown`、原因为 `eos_receipt_missing`；EOS 明确报告未知或非零退出时使用 `eos_stop_unconfirmed`。IsOS 绝不只凭普通日志或 PID 消失把未知升级为确认。

停止按逆**依赖**顺序，独立同层进程按确定的逆启动顺序。EGIA 和其他 EOS 消费者先于 EOS 提供者停止。EOS 是最后一个图子进程，可以在 MuJoCo 子进程仍存活时执行有界实体停止。bridge 和 config 按其声明依赖顺序停止；IsOS 不直接向 MuJoCo 发信号。同部署并发 Stop 加入同一操作并获取同一结果；Start 或 Restart 不得与 Stop 重叠。

## 状态、事件和错误

`GET /api/isos/v1/process-status` 新增相同形状的 `terminal`。现有 `state` 仍是实时事实：未知停止即使 `processes` 为空也为 `degraded`，`last_error` 是固定安全说明；确认停止为 `stopped`。失败的 Restart 或回滚在旧代停止确认前不能发布 `deployment_ready`，除非 EOS R06 契约已证明另一个明确的 fence 恢复方案。不得自动重放结果不确定的运动。替换前预检失败保留旧运行、PID、授权和当前运行日志范围。

IsOS 对已获准运行发出一次 `deployment_stopping` 和恰好一条终结事件，使用同一运行 ID 和请求关联 ID。正常确认停止以 `deployment_stopped` 结束；候选启动失败且其部分子进程安全停止时使用现有 `deployment_failed`；任何未知停止使用 `deployment_stop_unconfirmed`。终结事件使用脱敏固定 `fields:{"outcome":"confirmed"|"unknown","reason_code":null|<上述代码>}`。`deployment_stop_unconfirmed` 为 `level:error`，不替换 P07 固定首条就绪／故障锚点；`deployment_stopped` 为 `level:info`。子进程回收及终结状态发布后再写事件；日志写失败增加现有计数，不能延迟停止。EOS 自行以 `source:eos` 发停止事件，IsOS 不伪造。管理器会话预检拒绝仍使用 `deployment_preflight_failed`，不为旧运行发 `deployment_stopping`。

```json
{"schema_version":1,"ok":true,"data":{"stopped":["egia","eos"],"terminal":{"run_id":"r42","outcome":"unknown","reason_code":"eos_stop_unconfirmed","processes":[{"id":"egia","outcome":"confirmed","reason_code":null},{"id":"eos","outcome":"unknown","reason_code":"eos_stop_unconfirmed"}]}},"error":null}
```

[冻结用例](../../../tests/fixtures/dependent_stop/README_cn.md)规定停止、超时、提供者丢失、回滚和日志故障。R06 实现既须运行真实 EOS 与 MuJoCo，也须测试桩进程组；未选中子进程伪造的 `eos_stopped` 不能变成证明。仅靠桩不能宣称性能或实体安全。
