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

`outcome:"confirmed"` 要求所有选中的直接子进程组在期限内退出并被回收，且 EOS 的 R06 归属方停止证明成功。任何证明缺失、超时、信号失败、子进程意外退出、强制 KILL、EOS 实体停止未确认、进程组后代残留或授权注销失败均为 `unknown`；进程消失不能将其升级为确认。`reason_code` 仅在确认时为 null，否则取 `child_exit_unexpected`、`stop_signal_failed`、`stop_deadline_exceeded`、`stop_forced_kill`、`stop_group_unreaped`、`eos_stop_unconfirmed`、`eos_receipt_missing`、`control_grant_retire_failed` 或 `stop_result_persist_failed`。任一子进程结果未知，整体结果即未知。部分完成时仍为每个选中子进程提供结果。未知停止是已完成的管理器请求，`ok:true` 但 CLI 以非零状态退出；调用方必须检查 `terminal.outcome`，不能以 HTTP 状态判断安全。传输及归属方认证错误继续使用原有 `ok:false` 外层。

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

R06 `terminal` 对象单独存于 IsOS 私有运行状态，以归属方权限原子替换并 fsync 后才能回答确认。记录绑定部署名及运行 ID。持久化失败绝不能把未知升级为确认；最近获准运行已知时，`GET /process-status` 返回 `terminal.outcome:"unknown",reason_code:"stop_result_persist_failed"`。P07 的最近运行 ID 和锚点仍负责关联；终结记录不是新的图或日志流。

## 停止期限及 EOS 证明

拟议的 IsOS 预算使用一个单调时钟 `deployment.stop_timeout_ms` 总期限，类型为无符号整数、单位毫秒，缺省 **30,000**，只能在 `config/isos.toml` 配置为 **5,000..120,000**。拟议的 `deployment.eos_stop_grace_ms` 也是无符号整数毫秒，缺省 **20,000**，范围为 **1,000** 至 `stop_timeout_ms - 3,000`，用于在依赖者停止后为 EOS 保留时间。非 EOS 进程组共享至多 `min(8,000, stop_timeout_ms - eos_stop_grace_ms - 2,000)` 毫秒 TERM 宽限；至少 1,000 ms 留给依赖者，最后 2,000 ms 保留给有界强制停止、回收及终结结果持久化。缺失字段使用缺省值；格式错误、溢出或无效组合在管理器配置校验及新启动前以 `stop_budget_invalid`、`stage:"config"` 及 `path:"/deployment/stop_timeout_ms"` 或 `"/deployment/eos_stop_grace_ms"` 拒绝，活动运行保持不变。每次获准运行保留经过校验的预算，因此之后配置损坏不能阻止该运行的紧急 Stop。总期限从关闭停止准入开始，涵盖最终回收和结果持久化。到期时 IsOS 对仍存活的自有进程组发送 SIGKILL，有界回收直接子进程，将未解决组记为未知，且不等待日志。这些只属于归属方配置，图不得设置。SIGTERM 发往记录的子进程组，不能使用状态回复中的任意 PID；发信号前验证记录的子进程身份。直接子进程退出并不证明后代已结束。**EOS 优雅停止界限及预留公式仍待 EOS 归属方在 R06 设计评审中同意。**

EOS R06 需要一个绑定精确运行 ID 和观测 EOS PID 的**机器可检查的归属方终结结果**，证明 Dispatcher 关闭准入、所需底盘及机械臂停止得到确认或明确 fence、MuJoCo 子进程被回收、归属方 socket 已清理。进程退出码或一行普通 `eos_stopped` 本身不足以证明双实体确认。拟议传输为选中子进程管道接纳的有界结构化 EOS 终结收据，同时交叉检查退出状态和子进程身份；精确事件代码、字段、信号语义和 EOS 最大界限**仍待 EOS 归属方评审**。在该契约被评审通过前，IsOS 必须把 EOS 归为 `unknown`、原因为 `eos_receipt_missing`；EOS 明确报告未知或非零退出时使用 `eos_stop_unconfirmed`。IsOS 绝不只凭普通日志或 PID 消失把未知升级为确认。

停止按逆**依赖**顺序，独立同层进程按确定的逆启动顺序。EGIA 和其他 EOS 消费者先于 EOS 提供者停止。EOS 是最后一个图子进程，可以在 MuJoCo 子进程仍存活时执行有界实体停止。bridge 和 config 按其声明依赖顺序停止；IsOS 不直接向 MuJoCo 发信号。同部署并发 Stop 加入同一操作并获取同一结果；Start 或 Restart 不得与 Stop 重叠。

## 状态、事件和错误

`GET /api/isos/v1/process-status` 新增相同形状的 `terminal`。现有 `state` 仍是实时事实：未知停止即使 `processes` 为空也为 `degraded`，`last_error` 是固定安全说明；确认停止为 `stopped`。失败的 Restart 或回滚在旧代停止确认前不能发布 `deployment_ready`，除非 EOS R06 契约已证明另一个明确的 fence 恢复方案。不得自动重放结果不确定的运动。替换前预检失败保留旧运行、PID、授权和当前运行日志范围。

IsOS 对已获准运行发出一次 `deployment_stopping` 和恰好一条终结事件，使用同一运行 ID 和请求关联 ID。正常确认停止以 `deployment_stopped` 结束；候选启动失败且其部分子进程安全停止时使用现有 `deployment_failed`；任何未知停止使用 `deployment_stop_unconfirmed`。终结事件使用脱敏固定 `fields:{"outcome":"confirmed"|"unknown","reason_code":null|<上述代码>}`。`deployment_stop_unconfirmed` 为 `level:error`，不替换 P07 固定首条就绪／故障锚点；`deployment_stopped` 为 `level:info`。子进程回收及终结状态发布后再写事件；日志写失败增加现有计数，不能延迟停止。EOS 自行以 `source:eos` 发停止事件，IsOS 不伪造。管理器会话预检拒绝仍使用 `deployment_preflight_failed`，不为旧运行发 `deployment_stopping`。

```json
{"schema_version":1,"ok":true,"data":{"stopped":["egia","eos"],"terminal":{"run_id":"r42","outcome":"unknown","reason_code":"eos_stop_unconfirmed","processes":[{"id":"egia","outcome":"confirmed","reason_code":null},{"id":"eos","outcome":"unknown","reason_code":"eos_stop_unconfirmed"}]}},"error":null}
```

[冻结用例](../../../tests/fixtures/dependent_stop/README_cn.md)规定停止、超时、提供者丢失、回滚和日志故障。R06 实现既须运行真实 EOS 与 MuJoCo，也须测试桩进程组；未选中子进程伪造的 `eos_stopped` 不能变成证明。仅靠桩不能宣称性能或实体安全。
