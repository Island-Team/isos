# P06 部署事件设计

[English](../../../docs/isos/design/events.md) · [归属契约](../interfaces/events_cn.md) · [首次使用](../usage/get-started_cn.md)

**目标设计——尚未实现。** [事件接口](../interfaces/events_cn.md)持有 JSONL 字段、限制及生命周期规则；本页说明 IsOS 如何生成和接收。当前 `LogStore` 已创建私有轮转来源文件和 `events.log`，`manager_session_id()` 已为部分包事件标注身份。它也从两条子进程管道解析一部分 JSON，但丢失子进程的归属、节点、运行、关联及元数据字段。普通管理器事件没有运行上下文。P06 在保留现有日志读取方式的同时替换这条有损路径。

## 准入路径

API、CLI 与 Unix 管理器在工作前选定部署和请求关联。启动前包／图失败使用当前管理器会话 ID，即使旧部署仍在运行，防止一次 Apply 失败被误归于不相关的活动修订。全部 P04/P05 预检及秘密解析通过后，Start 在首个子进程启动前分配部署运行 ID，并记录在 Supervisor 中。Restart 只有进入替换阶段才建立候选运行；预检失败保留旧活动运行。替换失败使候选运行终结，若恢复旧规格则分配另一个恢复运行 ID。重复 Start 返回当前运行。管理器按归属契约向 EOS 和 EGIA 传入 ID，并为停止、子进程恢复和 status 保留它。一次运行可包含多个自动重启的子进程 PID；关联 ID 区分各次尝试。

`LogStore` 应在构造器验证精确信封并脱敏后才接受不可变事件对象。管理器自身调用点发出固定事件码、安全消息模板及类型化元数据；不得将任意异常链格式化进消息。P01 收据日志重新核对请求的 `{id,version}`，只从已验证收据复制可用摘要／提交字段。没有可信收据的包校验失败使用 null 来源信息，不能读取可能已被修改的文件。HTTP 与 CLI 失败共用事件构造器，因此在公共回复前仅出现一次失败，携带真实诊断码与图指针。schema-1 `log_append` 套接字只检查对端 UID，不能认证调用方声称的子进程角色。兼容适配器验证旧请求并保留 `accepted:true`，但按归属契约把来源、事件码与消息替换为固定的 `legacy_external` 管理器事件。它不分配活动部署运行 ID，也不发出子进程就绪事件。

`Supervisor` 知道所选子进程角色、PID、活动运行及允许的节点路径。它读取有界 stderr 行，严格解析无重复键的单个 JSON 对象，核对声称的来源／核心／运行与角色及活动运行，确保 EOS 子进程只指向已准入 EOS 节点、EGIA 子进程只指向已准入 EGIA 节点，并执行事件码专属的安全字段规则。所选 EOS runtime 可声明 `source:eos`，其 HTTP bridge 可声明 `source:bridge`，其 config server 可声明 `source:config`；所选 EGIA 子进程可声明 `source:egia`。原始 MuJoCo 诊断留在 EOS 归属目录；子进程不能冒充其他角色。编译进 IsOS 的 `event_policy` 表按审核后的 P06 归属契约固定每个角色的精确事件码、类型化字段、级别与可抑制类别，包括仅 EOS runtime 可发的 `eos_events_suppressed`；共享的 `events_suppressed` 只由 IsOS 发出。即使来源／核心／运行语法都正确，未知子进程事件码或子进程声称 IsOS 抑制码也被拒绝。EGIA 事件码在其归属目录审核并固定前保持关闭。普通 stdout 和非 JSON stderr 不进入 IsOS 共享流。错误或超长行被丢弃且不缓存剩余部分；下一条有效记录仍可成为首个就绪／失败事件。收集器保留有效的子进程时间、级别、事件码、节点、关联及类型化字段，并用自身子进程句柄写入 PID。EOS 和 EGIA 在共享流之外持有完整诊断。

使用一个有界接收队列，并为首次生命周期／失败／停止事件保留容量。队列满时先丢弃重复的低级别健康／画面诊断并增加计数；停止和安全执行器关断不等待日志磁盘写入。写入器只序列化一次，在现有轮转锁下把相同字节写入来源文件和统一文件，并在不递归记录的前提下显示写入故障。来源文件达到上限时仍保留统一副本。原始子进程行包括换行最多 8,064 字节；添加观察到的 PID 后，规范化存储行包括换行最多 8,192 字节。收集器拒绝规范化后超限的记录并增加有界 `log_ingest_rejected` 计数；原始超长行丢弃至换行后继续解析下一行。长消息和超长子进程行都不能由收集器悄悄截短。生产者在发出前缩短已脱敏的消息。UTF-8 截断必须保持字符边界。

只在同一个 `(run_id,source,node_path,event_code)` 键内去重相同的非关键周期事件。首个事件写入，重复项增加计数，不快于每 60 秒一次或在终结时发出摘要。状态变化、警告、错误、就绪与终结事件总是通过。计数和摘要不包含原始载荷。这限制日志量而不隐藏首事件。P07 持有轮转后的首事件查找和游标行为；实时健康以 status 为准。

## 验证切片

[冻结夹具](../../../tests/fixtures/jsonl_events/README_cn.md)覆盖管理器会话失败、精确包来源、新运行与重复 Start、Restart 预检及恢复、EOS／EGIA 来源绑定、安全字段保留、超长行丢弃及下一行恢复、秘密／提示词／画面拒绝、抑制、轮转和收集器故障下停止。单元测试使用假时钟、假写入器和 stub 子进程管道；边界回放使用真实 IsOS 管理器、所选 EOS JSONL stderr 及 EGIA JSONL stderr。回放检查同一已接收事件在 `eos.log` 与 `events.log` 中相同、首个失败在 CLI／API 回复前出现、status 包含活动运行 ID，以及停止后的终结事件。完整格式化、Clippy、测试和回放输出保存在忽略 Git 的 `logs/test/<run-id>/`，然后在此补充可复现命令和结果。本草案是契约证据，不是 P06 运行时证据。

2026-11-01 在 `7898735` 加本文档分支上完成设计校验：`python3 tests/fixtures/jsonl_events/check_cases.py` 检查 30 个冻结案例与原始／存储字节边界；12 个修改后的 Markdown 文件相对链接、JSON 解析及 `git diff --check` 通过。仓库检查 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test` 通过。完整输出位于忽略 Git 的 `logs/test/p06-design-20261101/`（`docs-gates.log`、`cargo-fmt.log`、`cargo-clippy.log`、`cargo-test.log`）。这些检查不能代替上述 P06 运行时和边界测试。

2026-11-01 审查修正校验：37 案例检查器新增旧套接字归属冒充、未知子进程事件码、跨归属节点及抑制码角色检查。链接、JSON、Python 编译、格式化、Clippy 和 Cargo 测试输出位于忽略 Git 的 `logs/test/p06-design-rereview-20261101/`。先前设计运行只有 30 案例，其日志仍保留作为原始基线。
