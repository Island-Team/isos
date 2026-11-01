# R06 部署依赖停止契约

[English](../../../docs/isos/interfaces/dependent-stop.md) · [设计](../design/dependent-stop_cn.md) · [首次使用](../usage/get-started_cn.md#r06-依赖停止目标) · [冻结用例](../../../tests/fixtures/dependent_stop/README_cn.md) · [R06 任务](../../../../Island-eos/docs/island/implementation-todo.md)

**目标设计，尚未实现。** IsOS 管理其直接子进程 EOS、EGIA、bridge 和 config 的进程组、同用户管理器及 loopback API 的响应、以及持久的未知停止意图。EOS 管理 Dispatcher 停止准入、底盘和机械臂实体停止及其直接 MuJoCo 子进程；MuJoCo 负责仿真器确认。IsOS 不发送执行器命令、不接管仿真器，也不把 PID 消失解释为安全停止。

## 调用方、边界与结果

同 UID 的 CLI 使用现有 schema-1 管理器 socket 的 `deployment_stop`；工作台使用 `POST /api/isos/v1/process-control` 和 `operation:"stop"`。附着的 `isos run` Ctrl+C、Restart、启动失败回滚、上游进程丢失后的恢复以及 `manager_shutdown` 使用同一个监管器停止原语。HTTP 请求的可选 `revision` 保持原有的旧编辑视图拒绝语义。图、软件包和 API 设置都不能延长 R06 停止期限。不增加保存图字段，也不改变 schema-1 外层响应。

```json
{"schema_version":1,"operation":"stop","revision":"7a8daee6b7db9105f5483506ffc1743625b63089710dfbf936622329c471016b"}
```

停止请求返回原有按逆依赖顺序排列的 `stopped` 数组，并新增 `terminal` 对象。`stopped` 只表示 IsOS 的直接子进程已被回收，不证明实体停止。重复 Stop 返回空 `stopped` 和最近获准运行保留的终结结果。尚未运行过的 home 返回 `terminal:null`。新 Run/Restart 获准后，该新运行在开始 Stop 前使用 `terminal:null`；替换前的预检拒绝保留旧运行及结果。

```json
{"schema_version":1,"ok":true,"data":{"stopped":["egia","bridge","config","eos"],"terminal":{"run_id":"r42","outcome":"confirmed","reason_code":null,"processes":[{"id":"egia","outcome":"confirmed","reason_code":null},{"id":"bridge","outcome":"confirmed","reason_code":null},{"id":"config","outcome":"confirmed","reason_code":null},{"id":"eos","outcome":"confirmed","reason_code":null}]}},"error":null}
```

`outcome:"confirmed"` 要求所有选中的直接子进程组在期限内退出并被回收，且 EOS 的 R06 归属方停止证明成功。任何证明缺失、超时、信号失败、子进程意外退出、强制 KILL、EOS 实体停止未确认、进程组后代残留或授权注销失败均为 `unknown`；进程消失不能将其升级为确认。`reason_code` 仅在确认时为 null，否则取 `child_exit_unexpected`、`stop_signal_failed`、`stop_deadline_exceeded`、`stop_forced_kill`、`stop_group_unreaped`、`eos_stop_unconfirmed`、`eos_receipt_missing`、`eos_receipt_invalid`、`eos_receipt_late`、`control_grant_retire_failed`、`stop_result_persist_failed` 或 `stop_interrupted`。任一子进程结果未知，整体结果即未知。部分完成时仍为每个选中子进程提供结果。未知停止是已完成的管理器请求，`ok:true` 但 CLI 以非零状态退出；调用方必须检查 `terminal.outcome`，不能以 HTTP 状态判断安全。传输及归属方认证错误继续使用原有 `ok:false` 外层。

| 字段 | 类型／单位 | 用途、缺省、校验及兼容性 |
| --- | --- | --- |
| `terminal` | 对象或 null／— | process-control、管理器回复及 `GET /process-status` 的新增结果。从未获准运行或新获准运行尚未开始 Stop 时为 null；同一管理器存活期间，Stop 或失败后保留。管理器重启后，最近运行保守地返回 `unknown` 与 `stop_interrupted`，即使此前实时 Stop 曾返回确认。记录缺失或无法读取但 P07 运行 ID 已知时，报告 `unknown`、`stop_result_persist_failed` 和空进程数组，不臆造子进程身份。若 P07 自身报告 `run_id_state:unavailable`，则 `terminal` 为 null、状态为 degraded 且禁止启动。日志不能重建确认。 |
| `terminal.run_id` | ASCII 部署运行 ID／— | P07 的最近获准 `r...` ID，1–64 位字母、数字、`_` 或 `-`；不能是管理器会话 ID 或调用方传入 ID。 |
| `terminal.outcome` | `confirmed` 或 `unknown`／— | 所有子进程停止证明；无隐式缺省。旧客户端可忽略新增对象，但不能把 `stopped` 当作实体安全证明。 |
| `terminal.reason_code` | 上述代码或 null／— | 停止顺序中第一项安全相关故障；确认时且仅确认时为 null。不包含原始 OS 错误或路径。 |
| `terminal.processes` | 对象数组／条 | 每个选中直接子进程一条，按逆依赖顺序，最大为获准进程数。MuJoCo 归 EOS 所有，不列为 IsOS 子进程。已提前退出的子进程除非此前已有归属方证明，否则为未知。有效运行记录恢复时从 `selected_processes` 重建数组，每个子进程均为未知并使用恢复原因。记录缺失或无效时数组为空且阻止启动；P07 只记录运行身份，因此不得臆造进程 ID。 |
| `terminal.processes[].id` | 注册进程 ID／— | 精确的 IsOS 进程 ID，1–48 位 ASCII 字母、数字、`_` 或 `-`，不是 PID。 |
| `terminal.processes[].outcome` | `confirmed` 或 `unknown`／— | 直接子进程组及归属方证明；无缺省。 |
| `terminal.processes[].reason_code` | 上述代码或 null／— | 固定原因；确认时且仅确认时为 null。 |

每次获准运行，IsOS 在 `0700` 私有目录下以 `0600` 普通文件创建 `data/stops/<deployment>/<run_id>.json`，不跟随符号链接。精确字段为 `schema_version:1`（无符号整数）、`deployment`（已验证 slug，1–48 位 ASCII 字母、数字、`_` 或 `-`）、`run_id`（精确 P07 部署运行 ID）、`phase`（`running` 或 `stopping`）、`outcome`（运行中为 null，停止中为 `unknown`）及 `selected_processes`（按计划逆依赖顺序排列的有界唯一注册进程 ID 数组）。文件中从不出现 `confirmed` 或 `terminal` 阶段。未知键、缺失字段、运行／部署不匹配、不安全元数据或非法转换均失败关闭。每次运行使用独立路径，旧运行的过晚写入不能覆盖新运行。P07 持久的最近运行 ID 选择对应记录；此文件不是新的图或日志流。

P07 持久发布新运行后、首个子进程启动前，IsOS 原子创建该运行的 `phase:running,outcome:null` 文件，并 fsync 文件和父目录。若两次发布之间崩溃造成不一致，P07 中的新运行恢复为未知且阻止启动；旧运行未知时，未经归属方评审的安全恢复不得替换。发出正常优雅停止信号前，IsOS 为同一运行原子替换为 `phase:stopping,outcome:unknown` 并 fsync 文件及父目录。此 fsync 是 Stop **唯一的持久安全提交点**。管理器崩溃、记录缺失或意图写入不确定均恢复未知。有界意图写入线程即使过晚完成，也只能将 `unknown` 写入该运行文件；停止归属方不会为等待它而错过紧急信号期限。这是持久化先于信号规则的唯一例外，结果始终为未知，即使后续实体清理成功。只有下述实时终结栅栏能将内存结果设为 `confirmed`。IsOS 永不持久写入确认，因此卡住的重命名或父目录 fsync 不会在调用方返回未知后悄悄升级结果。实时确认在管理器重启后有意降级为未知；操作员恢复须依靠经过评审的 EOS fence，而不能依靠历史事件。

## 停止期限及 EOS 证明

IsOS 预算使用单调时钟 `deployment.stop_timeout_ms` 总期限，类型为无符号整数毫秒，缺省 **30,000**、有效范围 **5,000..120,000**，只能在 `config/isos.toml` 设置。`deployment.eos_stop_grace_ms` 是无符号整数毫秒，缺省 **20,000**，有效范围从 **1,000** 至 `stop_timeout_ms - 3,000`，在依赖者停止后为 EOS 预留时间。非 EOS 进程组共享至多 `min(8,000, stop_timeout_ms - eos_stop_grace_ms - 2,000)` 毫秒的 TERM 宽限。关闭停止准入时开始总期限；`cleanup_at = deadline - 2,000 ms`。**最迟在 `cleanup_at`** 结束优雅等待，对仍存活且身份可安全确认的自有进程组发送 SIGKILL，将强制目标记为未知，并开始非阻塞回收轮询。最后窗口至多 1,500 ms 用于 KILL／回收，至少 500 ms 用于**实时**终结栅栏和响应；该终结路径没有文件系统调用。停止归属方与期限计时器在一个按运行 ID 绑定的内存栅栏上串行。归属方持有该栅栏时检查单调时钟以及所有子进程和 EOS 证明，然后原子选择终结值，并将同一值发布到共享状态和有界非阻塞响应队列。确认发布必须严格早于期限；若队列此时不能接收，则发布未知。到达或超过期限时计时器发布未知，恰好等于期限也归未知。此逻辑发布先于可能较晚完成的 OS 传输刷新，刷新不能改变已排队结果。结果发布后在该管理器生命周期内不可变，包括对第二个调用方；任何后台线程都不能修改。事件只能描述已经发布的值。最终仍未回收的子进程为未知，并留给后续有界尽力清理，不再向身份未固定的组发信号。字段缺失使用缺省值；格式错误、溢出或无效组合在管理器配置校验及新运行前以 `stop_budget_invalid`、`stage:"config"`、`path:"/deployment/stop_timeout_ms"` 或 `"/deployment/eos_stop_grace_ms"` 拒绝。每次获准运行保留经过校验的预算，之后配置损坏不能阻止紧急 Stop。这些是归属方设置，不是图字段。经评审的 EOS 15,000 ms 界限可容纳在缺省 20,000 ms 宽限及 30,000 ms 外层期限内；显式配置的较短宽限若早于 EOS 实际结束，过晚收据不得确认为成功。

只有 IsOS 保留尚未回收的已启动直接子进程**组长**时，向进程组发信号才有安全身份依据。Linux `waitid(..., WNOWAIT)` 可观察退出而不释放其 PID；就绪与提供者丢失轮询在最后一次组信号之前不能使用会回收的 `try_wait`。尚未回收的组长保留 PID／PGID，阻止负 PGID 被复用。IsOS 在启动时及每次 `kill(-pgid, signal)` 前核对所持组长的 `/proc/<pid>/stat` 启动 ticks、进程组和会话；无法读取或不匹配均属歧义。不再可能发组信号后才回收组长。pidfd 可识别组长，但在组长被回收后并不能固定 PGID。`EPERM`、身份意外变化、组长已被回收或组成员不确定时，以 `stop_group_unreaped` 失败关闭：不得向可能被复用的 PGID 发信号，也不得报告确认。管理器崩溃后，保存的 PID／PGID 不能用于发信号，因为未回收组长的持有关系已经消失；恢复结果为未知。直接子进程退出并不证明后代已离开组。

经评审的 [EOS R06 归属方契约](https://github.com/Island-Team/Island-eos/blob/refactor%28v2%29/runtime-main/docs/eos/interfaces/simulator-stop.md)要求从选中 EOS 子进程的**原始 stdout 管道**读取一条 `eos_terminal_stop` 收据。IsOS 使用专用严格解析器先于 P06 日志归一化读取，并经有界通道交给停止操作。P06 stderr JSONL 事件、`eos_stopped` 日志、单独的退出码、其他进程管道或历史收据均不能作为证明。IsOS 在所选 EOS 构建预检时检查 `managed-stop-v1`，但该 token 仅声明支持，不证明本次运行已经停止。EOS 为 MuJoCo 子进程及辅助进程关闭或重定向选中 stdout 的写端，避免后代长期持有 EOF。IsOS 对行装配设置上限，并要求一行之后且 EOS 退出后出现 EOF，以排除第二条收据；后代持有写端或在 EOS 宽限内没有 EOF 均为无效，不能延长外层期限。

收据必须恰好是一条以换行结尾的 JSON 对象，含换行至多 **4096 UTF-8 字节**，键唯一且无未知或缺失字段。`schema_version` 为整数 `1`；`type` 为 `eos_terminal_stop`；`run_id` 与 `active_revision` 分别精确等于获准的 P07 运行 ID 和已 Apply 的 SHA-256 修订；`eos_pid` 等于选中且未回收的 EOS 组长 PID。`backend_node` 等于所选规范短路径，例如 `eos/process/mujoco/sim`，而不能是 `/cores/...` 指针。`backend_pid` 与 `backend_pgid` 是正无符号 ID，单独会话的 MuJoCo 子进程满足 `backend_pgid == backend_pid`；仅在启动后未 spawn 的未知回滚中两者才可同时为 null。`generation_id` 是小写 32 位十六进制；仅在子进程从未就绪且结果未知时可为 null。`stop_epoch` 是规范 ASCII 十进制字符串；没有有效停止响应时可为 null，确认时必须存在，先前 epoch 比较由 EOS 负责。`admission_closed`、`backend_group_drained`、`backend_reaped` 和 `sockets_removed` 为布尔值。`parts` 以规范路径顺序恰好列出所选可停止的 MuJoCo 底盘和机械臂图节点，最多 64 个，路径和种类均唯一，每项只含 `node_path`、`kind`（`base` 或 `arm`）和 `outcome`（`stopped` 或 `unknown`）。只有该类执行器未被选中时才能省略；EOS 的整机器人 Stop 仍须证明两类。`outcome` 为 `confirmed` 或 `unknown`；仅确认时 `reason_code` 为 null，否则只能取 EOS 固定的 `stop_timeout`、`stop_socket_unavailable`、`stop_response_invalid`、`stop_part_unknown`、`backend_exited`、`backend_forced_kill`、`backend_group_unreaped`、`backend_reap_failed` 或 `socket_cleanup_failed`。不接纳原始路径、错误或密钥。

IsOS 确认要求唯一有效收据在 EOS 宽限和总期限之前从选中管道到达，声明 `confirmed`，所有所选部件均为 `stopped`，四项事实布尔值均为 true。随后观测 EOS 进程以**零码**退出，其身份可安全确认的直接子进程组排空，IsOS 回收所持组长，才可通过自己的实时终结栅栏发布确认。EOS 的 `backend_group_drained` 涵盖 MuJoCo 进程组全部成员，比组长回收更严格；IsOS 检查该字段的类型和值，但不能独立观测嵌套组，须依赖 EOS 归属方证明。收据缺失映射为 `eos_receipt_missing`；JSON 重复／未知／缺失键、格式错误、过大或不完整行、第二条收据、缺失 EOF、错误管道、运行身份／修订／后端／部件不匹配，或确认事实不一致，映射为 `eos_receipt_invalid`；超过任一期限的收据映射为 `eos_receipt_late`。EOS 明确报告未知或非零退出映射为 `eos_stop_unconfirmed`。即使日志或 PID 消失看似成功，部署终结仍为未知。EOS 从首次停止转换 t0 起，+1 秒关闭准入、+4 秒判定实体停止、+10 秒回收子进程、+13 秒发布收据、+15 秒退出；IsOS 不延长任何阶段。

停止按逆**依赖**顺序，独立同层进程按确定的逆启动顺序。EGIA 和其他 EOS 消费者先于 EOS 提供者停止。EOS 是最后一个图子进程，可以在 MuJoCo 子进程仍存活时执行有界实体停止。bridge 和 config 按其声明依赖顺序停止；IsOS 不直接向 MuJoCo 发信号。同部署并发 Stop 加入同一操作并获取同一结果；Start 或 Restart 不得与 Stop 重叠。

## 状态、事件和错误

`GET /api/isos/v1/process-status` 新增相同形状的 `terminal`。现有 `state` 仍是实时事实：未知停止即使 `processes` 为空也为 `degraded`，`last_error` 是固定安全说明；确认停止为 `stopped`。失败的 Restart 或回滚在旧代在同一管理器中实时停止确认前不能发布 `deployment_ready`，除非 EOS R06 契约已证明另一个明确的 fence 恢复方案。管理器重启后即使旧客户端曾看到确认，旧运行仍视为未知，须通过该归属方评审的恢复方案后才能再次启动。不得自动重放结果不确定的运动。替换前预检失败保留旧运行、PID、授权和当前运行日志范围。

IsOS 对已获准运行发出一次 `deployment_stopping` 和恰好一条终结事件，使用同一运行 ID 和请求关联 ID。正常确认停止以 `deployment_stopped` 结束；候选启动失败且其部分子进程安全停止时使用现有 `deployment_failed`；任何未知停止使用 `deployment_stop_unconfirmed`。终结事件使用脱敏固定 `fields:{"outcome":"confirmed"|"unknown","reason_code":null|<上述代码>}`。`deployment_stop_unconfirmed` 为 `level:error`，不替换 P07 固定首条就绪／故障锚点；`deployment_stopped` 为 `level:info`。仅在子进程回收及实时终结栅栏决定后才排队写事件；日志写失败增加现有计数，不能延迟停止。EOS 自行以 `source:eos` 发停止事件，IsOS 不伪造。管理器会话预检拒绝仍使用 `deployment_preflight_failed`，不为旧运行发 `deployment_stopping`。

```json
{"schema_version":1,"ok":true,"data":{"stopped":["egia","eos"],"terminal":{"run_id":"r42","outcome":"unknown","reason_code":"eos_stop_unconfirmed","processes":[{"id":"egia","outcome":"confirmed","reason_code":null},{"id":"eos","outcome":"unknown","reason_code":"eos_stop_unconfirmed"}]}},"error":null}
```

[冻结用例](../../../tests/fixtures/dependent_stop/README_cn.md)规定停止、超时、提供者丢失、回滚和日志故障。R06 实现既须运行真实 EOS 与 MuJoCo，也须测试桩进程组；未选中子进程伪造的 `eos_stopped` 不能变成证明。仅靠桩不能宣称性能或实体安全。
