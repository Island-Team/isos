# P06 部署事件设计

[English](../../../docs/isos/design/events.md) · [归属契约](../interfaces/events_cn.md) · [首次使用](../usage/get-started_cn.md)

**P06 已实现。** [事件接口](../interfaces/events_cn.md)定义 JSONL 字段、限制和生命周期规则。`LogStore` 创建私有轮转分类文件及 `events.log`；`Supervisor` 在接收结构化 stderr 事件前绑定所选子进程角色、运行 ID 和观察到的 PID。schema-1 旧 append 路径统一为不可信的管理器事件。P07 增加跨轮转游标保留。

## 准入路径

API、CLI 与 Unix 管理器在工作前选定部署和请求关联。启动前包／图失败使用当前管理器会话 ID，即使旧部署仍在运行，防止一次 Apply 失败被误归于不相关的活动修订。全部 P04/P05 预检及秘密解析通过后，Start 在首个子进程启动前分配部署运行 ID，并记录在 Supervisor 中。Restart 只有进入替换阶段才建立候选运行；预检失败保留旧活动运行。替换失败使候选运行终结，若恢复旧规格则分配另一个恢复运行 ID。重复 Start 返回当前运行。管理器按归属契约向 EOS 和 EGIA 传入 ID，并为停止、子进程恢复和 status 保留它。一次运行可包含多个自动重启的子进程 PID；关联 ID 区分各次尝试。

`LogStore` 在校验信封和安全字段后接收有界事件对象。管理器自身调用点发出固定事件码、安全消息模板及类型化元数据；不得将任意异常链格式化进消息。P01 收据日志重新核对请求的 `{id,version}`，只从已验证收据复制可用摘要／提交字段。没有可信收据的包校验失败使用 null 来源信息，不能读取可能已被修改的文件。管理器 Run/Restart 预检以及 HTTP 图 Validate/Apply 失败在公共回复前发出一次固定的 `deployment_preflight_failed` 事件。已知结构化诊断携带代码和有界图指针；未知错误不携带原始错误文本。schema-1 `log_append` 套接字只检查对端 UID，不能认证调用方声称的子进程角色。兼容适配器验证旧请求并保留 `accepted:true`，但按归属契约把来源、事件码与消息替换为固定的 `legacy_external` 管理器事件。它不分配活动部署运行 ID，也不发出子进程就绪事件。

`Supervisor` 知道所选子进程角色、PID、活动运行及允许的节点路径。它读取有界 stderr 行，严格解析无重复键的单个 JSON 对象，核对声称的来源／核心／运行与角色及活动运行，确保 EOS 子进程只指向已准入 EOS 节点、EGIA 子进程只指向已准入 EGIA 节点，并执行事件码专属的安全字段规则。所选 EOS runtime 可声明 `source:eos`，其 HTTP bridge 可声明 `source:bridge`，其 config server 可声明 `source:config`；所选 EGIA 子进程可声明 `source:egia`。原始 MuJoCo 诊断留在 EOS 归属目录；子进程不能冒充其他角色。编译进 IsOS 的 `event_policy` 表按审核后的 P06 归属契约固定每个角色的精确事件码、类型化字段、级别与可抑制类别，包括仅 EOS runtime 可发的 `eos_events_suppressed`；共享的 `events_suppressed` 只由 IsOS 发出。即使来源／核心／运行语法都正确，未知子进程事件码或子进程声称 IsOS 抑制码也被拒绝。EGIA 事件码已按经审核的 16 项归属策略夹具固定；未列出的事件码保持关闭。普通 stdout 和非 JSON stderr 不进入 IsOS 共享流。错误或超长行被丢弃且不缓存剩余部分；下一条有效记录仍可成为首个就绪／失败事件。收集器保留有效的子进程时间、级别、事件码、节点、关联及类型化字段，并用自身子进程句柄写入 PID。EOS 和 EGIA 在共享流之外持有完整诊断。

每条所选子进程管道由有界读取器接收，合格事件在共享轮转锁下直接写入。没有中间事件队列：可抑制的重复诊断在最多 256 键的表中计数，首次生命周期、失败、就绪与终结事件直接写入。停止和子进程回收独立于收集线程。相同序列化记录写入分类文件与统一文件；写入失败只增加 `log_write_failed`，不递归写日志。分类文件达到数量上限时仍保留统一副本。私有原始子进程 stdout/stderr 也轮转。原始行（含换行）至多 8,064 字节；加入观察到的 PID 后的存储行至多 8,192 字节。拒绝行增加 `log_ingest_rejected` 及有界原因计数。超长原始行丢弃至换行后继续解析下一事件。生产者只缩短已脱敏消息。

只在同一 `(run_id,source,node_path,event_code)` 键内去重允许抑制的非关键重复诊断。首条写入，重复项计数，摘要不快于每 60 秒一次或在终结时发出。首条警告以及所有状态变化、就绪、失败与终结事件均通过。P07 持有轮转后的首事件查找及游标；实时健康以 status 为准。

## 验证切片

[冻结夹具](../../../tests/fixtures/jsonl_events/README_cn.md)覆盖管理器会话失败、包来源、运行身份、所选子进程角色绑定、错误及超长行、不安全载荷拒绝、抑制、轮转和写入失败下停止。单元测试使用 stub 子进程管道及所选进程边界测试；合并后的真实 EOS runtime 二进制通过收集器回放启动失败路径。EGIA 经审核的 16 个事件码与归属夹具核对；合并后的 EGIA 启动器作为所选受管子进程回放，其接收事件在分类与统一日志间对比。完整输出保存于忽略 Git 的 `logs/test/<run-id>/`。

2026-11-01 在 `7898735` 加本文档分支上完成设计校验：`python3 tests/fixtures/jsonl_events/check_cases.py` 检查 30 个冻结案例与原始／存储字节边界；12 个修改后的 Markdown 文件相对链接、JSON 解析及 `git diff --check` 通过。仓库检查 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test` 通过。完整输出位于忽略 Git 的 `logs/test/p06-design-20261101/`（`docs-gates.log`、`cargo-fmt.log`、`cargo-clippy.log`、`cargo-test.log`）。这些检查不能代替上述 P06 运行时和边界测试。

2026-11-01 审查修正校验：37 案例检查器新增旧套接字归属冒充、未知子进程事件码、跨归属节点及抑制码角色检查。链接、JSON、Python 编译、格式化、Clippy 和 Cargo 测试输出位于忽略 Git 的 `logs/test/p06-design-rereview-20261101/`。先前设计运行只有 30 案例，其日志仍保留作为原始基线。

2026-11-01 实现校验，基于 IsOS `16d189f`：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`EOS_P06_RUNTIME_BIN=/home/ashex/projects/Island-eos/target/debug/aether-runtime EGIA_P06_LAUNCHER=/home/ashex/projects/Island-egia/bin/egia EGIA_P06_PYTHON=/home/ashex/projects/Island-egia/.venv/bin/python cargo test --no-fail-fast`、`cargo build --all-targets`、`python3 tests/fixtures/jsonl_events/check_cases.py`、`python3 tests/fixtures/reader_features/check_docs.py` 和 `git diff --check` 均通过。Cargo 测试为 150 个单元、5 个 CLI、1 个已移除命令及 9 个边界测试；夹具检查为 37 案例及两个字节边界。EOS runtime 二进制由已合并 EOS P06 runtime-main `f22d6e7c` 构建；EGIA 启动器和 Python 来自已合并 EGIA P06 master `a03981f`。完整命令输出位于忽略 Git 的 `logs/test/20261101-p06-jsonl-events-implementation/`（`fmt.log`、`clippy.log`、`tests.log`、`build.log`、`fixture.log`、`docs.log`、`diff.log`）。这包括真实 EOS 启动失败与真实 EGIA 受管启动器收集器回放；独立实现审查仍待完成。
