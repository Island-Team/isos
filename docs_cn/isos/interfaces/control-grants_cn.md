# R03 托管 EOS 控制授权

[English](../../../docs/isos/interfaces/control-grants.md) · [设计](../design/control-grants_cn.md) · [首次使用](../usage/get-started_cn.md#r03-托管控制授权重放) · [固定用例](../../../tests/fixtures/control_grants/README_cn.md)

**目标契约，尚未实现。** IsOS 负责为托管部署运行签发和传递授权；EOS 负责验证、准入、Dispatcher 和持久结果。[EOS gRPC 契约](https://github.com/Island-Team/Island-eos/blob/refactor%28v2%29/runtime-main/docs/eos/interfaces/grpc.md#r03-governed-actuator-admission-target-contract)定义请求元数据和操作语义。图连线、软件包、生成配置、浏览器请求及客户端自称的阶段均不能授予执行器权限。信任边界是本机操作系统用户；本契约不声称能区分同 UID 的不同进程。

## 输入、文件和接收方

IsOS 仅在软件包、读取器、所有者渲染、二进制和密钥预检完成后，接受精确的已保存 `0.1.0` 图修订与**显式选中的 P04 schema-2 V2 代**。已保存字节的 SHA-256 是 `active_revision`；EOS 投影摘要是另一个身份。签发前，IsOS 验证选择条目、`data/generated/<deployment>/v2/<revision>/<generation>/manifest.json` 以及其中唯一列出的常规文件 `eos/graph.json`。即使图为 `0.1.0`，缺少选择或回退到 G03 也不得签发授权或停止活动子进程。旧 `1.0.0` 图与 G03 快照不获控制授权包。Validate、Apply 和迁移均不签发授权。

每个新的部署 `run_id` 由 IsOS 从操作系统 CSPRNG 独立生成**两个 32 字节随机值**。在私有运行时根目录下创建 `run/control-grants/<deployment>/<run_id>/`，目录权限为 `0700`，路径不得含符号链接或已存在的目标。每个授权文件是权限 `0600` 的常规文件，内容恰为 64 个小写十六进制字符及一个换行符，解码后为 32 字节。独立的 `0600` 验证 JSON 不包含授权原文：

```json
{
  "schema_version": 1,
  "run_id": "<精确的部署运行 ID>",
  "active_revision": "<64 位小写十六进制已保存字节 SHA-256>",
  "egia_control_sha256": "<解码后的 EGIA 授权字节的 SHA-256>",
  "operator_control_sha256": "<解码后的操作员授权字节的 SHA-256>"
}
```

运行目录中的文件名为 `egia-control.grant`、`operator-control.grant` 和 `verifier.json`。IsOS 不跟随链接、不覆盖已有文件地写入，使用前 fsync 文件及目录；重用时检查所有者 UID、常规文件类型、权限和精确内容。路径冲突、随机值不足、不安全路径、文件格式错误或验证信息不一致，都必须在 EOS 执行器写入就绪之前失败。状态和诊断可以报告运行 ID、修订和稳定的脱敏错误码，不得报告授权、授权摘要、文件内容或 bearer 元数据。

| 接收方和通道 | 私有启动输入 | 传递的权限 |
| --- | --- | --- |
| 托管 EOS 运行时 | `EOS_CONTROL_GRANTS_FILE=<verifier.json 的绝对路径>` | 仅验证摘要及精确运行/修订，不含授权原文。 |
| 受监管的 EGIA 子进程 | `EGIA_EOS_CONTROL_GRANT_FILE=<egia-control.grant 的绝对路径>` | EGIA 角色授权；托管客户端通过 EOS gRPC bearer 元数据发送。 |
| 受监管的 EOS HTTP bridge | `AETHER_BRIDGE_CONTROL_GRANT_FILE=<operator-control.grant 的绝对路径>` | bridge 所属的操作员角色授权，用于所有者控制路径及允许的受监管 stop/estop。不得进入 HTML、JavaScript、浏览器载荷或公开 HTTP 响应。 |

管理器只在指定的**已选**子进程 spawn 时注入对应变量，覆盖同名的图 `settings.env`，并从其他子进程的继承环境中移除这些变量。托管执行器部署要求已选 `eos` 运行时；`egia` 与 `bridge` 进程规格是可选的，未配置时不得阻止有效的仅 EOS 启动，也不得把其授权路径交给其他子进程。已选接收方是 ID 与角色匹配的已验证进程规格（`eos`/`eos`、`egia`/`egia` 或 `bridge`/`eos`）。IsOS 在移除或停止活动部署前检查完整候选规格集合及接收方映射；必需的 EOS 或已声明的已选接收方遗漏均在预检中失败。`deployment_starting` 之后的注入故障使候选运行终止，可能需要另一个恢复运行。图作者不能通过 `env_from`、`args` 或图设置选择路径或角色。EOS 允许 `egia-control` 执行受监管的 gRPC 运动，但不允许其使用私有浏览器路径；允许 `operator-control` 通过 bridge `primitive_typed` 运动及受监管的 gRPC stop/recovery，但拒绝其 gRPC 运动。两个角色均可执行已认证的 `safety.estop`，并且只能查询、观察或取消自身的持久操作。bridge 在外层所有者 socket 字段发送 `authority_grant`，浏览器不能提供此字段。EOS 在准入处检查角色与方法。持有运行期授权仅证明本地同 UID 边界内签发的角色，不证明 EGIA 安全阶段；固定阶段路径属于 A02 门槛。

## 运行生命周期与可观察失败

Run/Restart 在替换活动子进程之前完成现有 P04/P05 预检，解析所有 `env_from` 值。它们也在停止前拒绝已选 EOS runtime/bridge/config `settings.args` 中图作者写入的 `--run-id`/`--run-id=...` 及其他所有者注入标志（`--backend-release-sha256`、`--backend-payload-sha256`、`--log-dir`）；现有生成 profile 检查拒绝重复的 `--profile`。IsOS 在停止旧进程前准备候选授权包，但只把它交给候选子进程。幂等 Start 保留当前 `run_id`、修订和授权包。同一部署运行内 EOS、EGIA 或 bridge 的自动子进程重启重用授权包，以便按原 ID 查询丢失响应的操作。显式 Restart 即使修订未变，也创建新的运行 ID 和授权。替换失败使候选运行终止；如恢复旧进程规格，恢复使用另一个新运行 ID 和授权包。旧子进程被回收后才撤销旧授权包；预检失败不影响旧子进程和授权包。Stop 在回收子进程后撤销授权包。管理器重启创建新的部署运行，不暗中恢复旧授权。

EOS 仅在**同时**收到所选 IsOS `v2/<revision>/<generation>/eos/graph.json` profile 路径和 `EOS_CONTROL_GRANTS_FILE` 时进入托管准入；缺任一项均在监听器绑定前启动失败。两者皆无时，即使存在 `--run-id`，也只能以独立只读方式运行。托管准入中，EOS 在公开写入就绪前，比较验证文件中的精确 `run_id` 与 IsOS 提供的有效 `--run-id` 实参、`active_revision` 与已准入 schema-2 manifest 修订。它还独立验证所选 profile 的 manifest 条目和字节。授权包缺失、过期、不安全或不匹配时，执行器写入不得就绪，也不得发出可写能力的 EOS ready 声明。仅绑定 socket 不代表子进程就绪。现有 schema-1 管理器和 HTTP 信封不变。`deployment_starting` **之前**的授权拒绝以请求关联号在管理器会话 ID 下写入脱敏 `deployment_preflight_failed`；旧活动部署及 P07 当前运行范围保持不变。`deployment_starting` **之后**的故障属于候选部署运行，并由其终结故障事件结束。两种情况下已保存图字节均不改变；活动修订只在成功替换或不同的恢复运行后变化。

为 IsOS 启动边界提出以下稳定代码：`control_grant_unsafe_path`、`control_grant_invalid_file`、`control_grant_mismatch`、`control_grant_issue_failed` 和 `control_grant_delivery_failed`。各代码使用 `stage:"spawn"` 及空的图 `path`，因为这些文件属于运行时状态，不是图字段。`control_grant_unsafe_path` 覆盖所有权、权限、符号链接和非常规文件；`control_grant_invalid_file` 覆盖形状与长度；`control_grant_mismatch` 覆盖运行、修订或摘要不一致；`control_grant_issue_failed` 覆盖随机生成和持久发布；`control_grant_delivery_failed` 覆盖停止前检测到候选计划遗漏必需的 EOS 或已声明的已选 EGIA/bridge 接收方，以及候选启动后的实际注入故障。后一种情况不能保证旧 PID 不变；它终止候选运行，并在可能时启动不同的恢复运行。可选 EGIA 或 bridge 规格不存在并非错误。图作者写入的所有者保留标志另返回 `control_grant_reserved_arg`，使用 `stage:"graph"` 与精确的 `/cores/isos/layers/process/groups/component/nodes/<id>/settings/args/<index>` 指针；`--run-id=...` 指向其数组元素。重复 `--profile` 仍由 P04 生成 profile 检查负责。CLI 非零退出，API 沿用现有错误信封。错误文案只说明修复动作，不含授权字节或文件内容。运行时操作鉴权错误属于 EOS gRPC 状态码，不属于 IsOS 图诊断。

实现关闭前，使用干净的 `ISOS_HOME`、实际所选 EOS 和受监管 EGIA/bridge stub 重放[固定用例](../../../tests/fixtures/control_grants/README_cn.md)。检查文件权限、各子进程环境隔离、预检拒绝后的活动 PID 不变、运行轮换、同运行自动重启恢复、新运行旧授权拒绝、就绪前失败和脱敏日志。完整输出保存于 `logs/test/<run-id>/`；运行 IsOS 格式、严格 Clippy、测试与构建，以及 EOS R03 边界重放。授权测试不能证明 MuJoCo 实体运动；R05/R06 负责其 epoch fence 和 stop/reap 证据。
