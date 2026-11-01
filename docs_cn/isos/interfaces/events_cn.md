# 共享部署事件

[English](../../../docs/isos/interfaces/events.md) · [配置与日志存储](configuration_cn.md) · [本地日志 API](local-api_cn.md) · [首次使用](../usage/get-started_cn.md) · [P06 任务](../../../../Island-eos/docs/island/implementation-todo.md)

**目标设计——尚未实现。** IsOS 持有管理器会话、部署运行身份、私有分类日志、统一 `events.log` 与所选子进程事件接收。EOS 和 EGIA 持有各自事件的含义。IsosUI 读取 IsOS 事件流；不能仅凭事件推断实时健康。P06 增加共享的已脱敏 JSON Lines（JSONL）信封，P07 增加跨轮转的游标留存与首事件导航。

## 生产者与收集器边界

一条记录由一个 UTF-8 JSON 对象和末尾 `\n` 组成，原始子进程行（包括换行）最多 8,064 字节，为 IsOS 规范化和观察到的 PID 留出 128 字节；存储行（包括换行）最多 8,192 字节。生产者先清理 `message`，然后在 UTF-8 边界缩短它，直到完整序列化记录满足整行限制；单个字段满足限制也不能豁免整行限制。IsOS 只接收来自已认证管理器或所选直接子进程管道的有界有效记录，并写入 `data/logs/<deployment>/<source>.log` 与统一 `events.log`；目录权限 `0700`，文件权限 `0600`。来源文件达到上限时，新来源仍可仅写统一流。普通 stdout/stderr、无效 JSON、未知顶层字段、重复 JSON 键、超长行和未注册的子进程事件码不进入 IsOS 事件流；收集器丢弃超长行直至换行，然后继续解析下一条。拥有核心自行保存完整诊断。

| 字段 | JSON 类型／单位 | 用途、缺省、校验与兼容 |
| --- | --- | --- |
| `timestamp` | RFC 3339 UTC 字符串／毫秒 | 必需事件时间。IsOS 为管理器事件生成；保留所选子进程的有效时间。拒绝非 UTC 或格式错误的子进程值。 |
| `level` | `trace`、`debug`、`info`、`warn`、`error`／— | 必需级别；保留现有过滤顺序。失败为 `error`，安全拒绝可为 `warn`。 |
| `source` | ASCII 字符串／来源 ID | 必需，1–48 个字母、数字、`_` 或 `-`。Supervisor 将 EOS runtime 的 `eos`、EOS HTTP bridge 的 `bridge`、EOS config server 的 `config` 与 EGIA 的 `egia` 绑定到各自所选直接子进程角色。原始 MuJoCo 子进程诊断留在独立的 EOS 归属日志；`mujoco` 留给将来已验证的共享生产者。任意子进程不能冒充 `isos`、`packages` 或其他子进程。 |
| `core` | `isos`、`eos`、`egia`／归属 ID | 必需运行时所有者。IsOS workbench、包、图和独立服务事件使用 `isos`；EOS 后端生命周期事件使用 `eos`。收集器从所选进程确定预期所有者，拒绝不一致值。 |
| `node_path` | JSON Pointer 字符串或 null／已保存图路径 | 可选；缺省规范化为 null。非空值是精确终止于 `/cores/<core>/layers/<layer>/groups/<group>/nodes/<node>` 的规范转义已保存图指针，最多 512 UTF-8 字节，指向事件责任节点；尾部 `/package` 等成员指针被拒绝；已准入图的段语法排除 ID 中的 `/` 与 `~`，指针转义不能使无效 ID 合法；EOS 在发出前展开内部四段节点路径。不允许本地文件路径。 |
| `event_code` | 小写 ASCII 字符串／稳定事件码 | 必需，1–64 个字母、数字或 `_`。生产者持有自身事件码清单；IsOS 只接收从已审核归属契约固定到版本化角色策略中的事件码，不把任意子进程文本当成事件码。旧日志缺失此字段仍可读；P06 新记录总是写入。 |
| `run_id` | ASCII 字符串／运行或管理器会话 ID | 必需，1–64 个字母、数字、`_` 或 `-`。`m...` 表示启动前的管理器会话，`r...` 表示一次部署运行。收集器填充或核对 Supervisor 指定值，不让子进程选择另一运行。旧日志缺失仍可读。 |
| `correlation_id` | ASCII 字符串或 null／不透明请求 ID | 可选；缺省规范化为 null。语法和长度与 `run_id` 相同。关联请求、子进程启动、操作或失败，不嵌入用户文字或凭证。 |
| `message` | 字符串／UTF-8 字节 | 必需简短说明，单独最多 8,192 字节，并受整行限制进一步缩短。内部消息使用经审核模板；子进程消息必须通过该角色的安全消息规则。不得保存原始异常链、提示词、画面、凭证或环境值。 |
| `fields` | 对象／有界元数据 | 可选；缺省规范化为 `{}`。最多 12 个允许的标量键，序列化后最多 4 KiB；键长 1–48 个 ASCII 字母、数字、`_` 或 `-`；字符串值最多 256 UTF-8 字节且不含控制字符。不允许嵌套值、任意文本、路径、原始载荷或秘密。每个事件码有允许的类型化键集合。 |
| `pid` | 正整数或 null／操作系统进程 ID | 可选；缺省规范化为 null。原始子进程信封不含 `pid`；IsOS 校验后添加观察到的直接子进程 PID。管理器事件使用管理器 PID。子进程不能伪造。这是进程元数据，不是就绪证明。 |

### 角色策略与旧套接字

IsOS 发行版在目标 `src/runtime/event_policy.rs` 中携带经过审核的版本化角色策略。此表是收集器对事件码、类型化字段、级别、节点归属和可抑制类别的允许清单；它与所选 EOS／EGIA 归属契约固定对应，并用相应发行夹具测试。子进程不能通过 JSON、capability 回复或图值自行扩展。初始 EOS 策略从已审核的 EOS P06 归属事件契约复制精确事件码：runtime 的 `eos_starting`、`eos_ready`、`eos_start_failed`、`eos_stopping`、`eos_stopped`、`eos_package_admitted`、`eos_package_provenance_unavailable`、`eos_backend_ready`、`eos_backend_lost`、`eos_stop_unconfirmed` 和仅 runtime 可发的 `eos_events_suppressed`；bridge 的 `bridge_starting`、`bridge_ready`、`bridge_start_failed`、`bridge_stopping`、`bridge_stopped`；config 的 `config_starting`、`config_ready`、`config_start_failed`、`config_stopping`、`config_stopped`。EOS 的 `node_path` 必须指向已准入 EOS 节点。EGIA 角色在已审核的 P06 归属目录固定到同一表前，对未列出的事件码保持关闭；仅有所选 EGIA 子进程和有效 `run_id` 不授予任意事件准入。只有 IsOS 构造 `source:isos`、`core:isos` 的 `events_suppressed`，字段限于有界的 `suppressed_source`、`suppressed_event_code` 和 `count`；子进程声称此码时被拒绝。角色策略还拒绝仅对另一所选进程有效的事件码／字段组合。

schema-1 管理套接字 `log_append` 只认证同 UID 本地 peer，无法证明其是所选 EOS、EGIA、bridge 或 config 进程。调用方传入的 `role`、`source`、`event_code`、`message` 不能建立归属或就绪。P06 保留历史请求形状，对语法有效且非保留来源的请求继续返回 `{"accepted":true}`，包括历史 `source:"eos"` 请求；但所有此类追加都规范化为 `source:legacy_external`、`core:isos`、固定 `event_code:legacy_external_event`、固定安全 `message:"Legacy external event received"`、调用方级别、管理器会话 `run_id`、新生成的请求关联和管理器 PID。不可信的请求来源、事件码与消息不会写入共享流。已有保留来源拒绝规则仍有效。`legacy_external_event` 不影响所选子进程就绪或部署健康。经过验证的所选子进程 stderr 管道是 P06 向权威 `eos`、`bridge`、`config` 或 `egia` 来源写记录的唯一途径。

启动前的 IsOS 包失败使用管理器会话和请求关联。`null` 表示收据值不可用，不得猜测：

```json
{"timestamp":"2026-11-01T10:00:00.000Z","level":"error","source":"packages","core":"isos","node_path":"/cores/eos/layers/process/groups/mujoco/nodes/sim","event_code":"package_load_failed","run_id":"m42-abc","correlation_id":"p3","message":"Verified package input changed; relink the exact release","fields":{"package_id":"island/mujoco-sim","package_version":"0.1.1","source_kind":"local_link","release_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","payload_sha256":null,"stage":"payload"},"pid":42}
```

启动后，所选 EOS 子进程在 stderr 写入 `eos_ready` 事件。原始 JSONL 对象不含 `pid`；收集器保留经过校验的归属、节点、运行和关联字段，并写入观察到的子进程 PID。存储后的事件为：

```json
{"timestamp":"2026-11-01T10:00:01.000Z","level":"info","source":"eos","core":"eos","node_path":null,"event_code":"eos_ready","run_id":"r7-cde","correlation_id":null,"message":"EOS runtime ready","fields":{"state":"ready"},"pid":4242}
```

## 运行身份与生命周期

管理器启动时创建一个会话 ID。包安装／链接、草稿 Validate、Apply、版本检查及被拒绝的 Run/Restart 预检使用该 ID；可识别请求还分配新的 `correlation_id`。即使旧部署仍在运行，这些也是管理器事件。首次通过预检的 `deployment_start` 在图、包、读取器、生成版本和秘密全部通过后、首个子进程启动前分配新的部署 `run_id`；同一代际全部子进程共享。对已运行代际重复 Start 返回现有 ID，不重复启动／就绪摘要。Restart 预检失败不改变活动运行，失败事件归于管理器会话。已开始替换子进程的 Restart 使用新运行 ID；替换失败时该尝试有终结失败事件，若恢复旧规格，恢复过程使用另一个 ID。已准入部署运行内部的自动子进程重启保留原运行 ID，每次尝试使用新关联 ID。Stop 和终结结果使用活动运行 ID。IsOS status 才是当前就绪的权威，而非最新日志或 PID。

IsOS 在首个子进程前记录一条 `deployment_starting`，在全部必需就绪检查后记录一条 `deployment_ready`，之后只记录状态变化，并为每次运行记录一条终结 `deployment_stopped` 或 `deployment_failed`。EOS 和 EGIA 各自在自身前提完成后记录一次启动／就绪摘要，随后记录变化和终结结果。监听端口、已启动 PID 或探针成功都不是归属就绪事件。启动前失败在 API／CLI 返回已脱敏错误前使用管理器会话 ID 记录；分配运行 ID 后的子进程失败使用该运行 ID。无法持久化的事件通过有界丢弃／错误计数可见；日志故障不能阻塞执行器停止或进程回收。

## 所选 EOS 与 EGIA 交接

IsOS 向所选 EOS runtime 传入 `--run-id <id>`。对经过验证的受管 MuJoCo 子进程，还要同时传入 `--backend-release-sha256 <64 位小写十六进制>` 和 `--backend-payload-sha256 <64 位小写十六进制>`；没有该子进程时两项都不传。精确 `{id,version}` 来自已准入的 `node.package`，IsOS 在传入摘要前与 P01 收据重新核对。EOS 在打开 socket 或设备前校验这些参数，自行监督仿真器，并在 stderr 写入已脱敏 JSONL 生命周期事件。IsOS 持有最终顶层 `eos.log` 和 `events.log`；EOS 的 `--log-dir` 指向独立私有的 `data/logs/<deployment>/owner/eos/`，用于原始仿真器和归属诊断。摘要只证明来源，不授予执行器权限。IsOS 不替 EOS 安装包，也不读取仿真器 socket。

所选受管 EGIA 子进程通过非秘密环境绑定 `ISOS_RUN_ID=<deployment-run-id>` 接收同一 ID。EGIA 在启动前校验；格式错误时关闭启动准入，独立运行且没有此绑定时自行生成本地运行 ID。只有 IsOS Supervisor 核对活动运行及允许的 `egia` 来源后，EGIA 结构化事件才从所选管道进入统一流；不一致的子进程 ID 被拒绝。EGIA 监听器或 PID 不表示 EOS 发现和安全 gate 已就绪。IsOS 不把图边转换成 EGIA 命令。

## 脱敏、抑制与错误

发出者构造固定事件码和安全消息模板。IsOS 在存储前再次执行进程角色的事件码及字段允许清单、类型化值、精确包 pin／摘要形状和整行上限。包事件的 `package_id` 与 `package_version` 取自请求或已验证的精确 pin；有收据时，`source_kind`、`source_commit`、`asset_sha256`、`release_sha256` 和 `payload_sha256` 来自重新核对的收据。缺少来源信息保留 null。原始图、命令参数、完整异常、模型提示词、画面、秘密名称／值、任意环境变量或绝对源路径绝不进入 `message` 或 `fields`。在写任一事件流前拒绝或清理不安全输入；不能依赖 UI 过滤做脱敏。

状态首次就绪、警告、失败与终结事件不被抑制。重复且非关键的周期健康或帧诊断按 `(run_id,source,node_path,event_code)` 计数，以不快于每 60 秒一次以及终结时的一条有界 `events_suppressed` 摘要替代。同一故障的高频重复日志不能逐出首次故障。收集器内存有界，并为生命周期／失败事件保留通道；过载时先丢弃低级别重复，增加可见计数，不能声称被丢弃事件已写入。单次日志写入或收集器故障不能延迟停止。P07 增加当前运行游标导航及跨文件轮转的留存；P06 保持现有私有轮转上限并及时发出首事件。

`GET /api/isos/v1/logs?source=all&limit=100` 与 `isos logs all --level info` 通过现有 API／CLI 读取已准入事件。被拒绝的子进程行，包括规范化存储记录超过 8,192 字节的行，不是 API 错误；它增加带有有界原因码、且不含原始行的可见 `log_ingest_rejected` 计数。IsOS 写入失败在管理器 status／诊断中显示 `log_write_failed`，不能递归记录。现有 schema-1 `log_append` 仍限同 UID 所有者调用，接受历史小请求，但遵循上述不可信旧请求规范化；成功回复仍含 `{"accepted":true}`。[P06 夹具](../../../tests/fixtures/jsonl_events/README.md)冻结正例、反例、高频和停止案例；关闭任务前须回放真实 EOS 与 EGIA 子进程流。
