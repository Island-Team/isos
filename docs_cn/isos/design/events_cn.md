# P06 部署事件设计

[English](../../../docs/isos/design/events.md) · [归属契约](../interfaces/events_cn.md) · [首次使用](../usage/get-started_cn.md)

**P06 已实现。** [事件接口](../interfaces/events_cn.md)定义 JSONL 字段、限制和生命周期规则。`LogStore` 创建私有轮转分类文件及 `events.log`；`Supervisor` 在接收结构化 stderr 事件前绑定所选子进程角色、运行 ID 和观察到的 PID。schema-1 旧 append 路径统一为不可信的管理器事件。P07 增加跨轮转游标保留。

## 准入路径

API、CLI 与 Unix 管理器在工作前选定部署和请求关联。启动前包／图失败使用当前管理器会话 ID，即使旧部署仍在运行，防止一次 Apply 失败被误归于不相关的活动修订。全部 P04/P05 预检及秘密解析通过后，Start 在首个子进程启动前分配部署运行 ID，并记录在 Supervisor 中。Restart 只有进入替换阶段才建立候选运行；预检失败保留旧活动运行。替换失败使候选运行终结，若恢复旧规格则分配另一个恢复运行 ID。重复 Start 返回当前运行。管理器按归属契约向 EOS 和 EGIA 传入 ID，并为停止、子进程恢复和 status 保留它。一次运行可包含多个自动重启的子进程 PID；关联 ID 区分各次尝试。

`LogStore` 在校验信封和安全字段后接收有界事件对象。管理器自身调用点发出固定事件码、安全消息模板及类型化元数据；不得将任意异常链格式化进消息。P01 收据日志重新核对请求的 `{id,version}`，只从已验证收据复制可用摘要／提交字段。没有可信收据的包校验失败使用 null 来源信息，不能读取可能已被修改的文件。管理器 Run/Restart 预检以及 HTTP 图 Validate/Apply 失败在公共回复前发出一次固定的 `deployment_preflight_failed` 事件。已知结构化诊断携带代码和有界图指针；未知错误不携带原始错误文本。请求分发前只分配一次关联 ID，包加载详情与预检摘要复用它；含恶意控制字符的诊断指针被省略，但摘要仍写入。首次 Start 失败会移除已停止的 Supervisor，让重试获得新的运行 ID。Restart 在写旧运行停止事件前先停止并回收旧 Supervisor；日志存储初始化失败增加可见的写入健康计数。schema-1 `log_append` 套接字只检查对端 UID，不能认证调用方声称的子进程角色。兼容适配器验证旧请求并保留 `accepted:true`，但按归属契约把来源、事件码与消息替换为固定的 `legacy_external` 管理器事件。它不分配活动部署运行 ID，也不发出子进程就绪事件。

`Supervisor` 知道所选子进程角色、PID、活动运行及允许的节点路径。它读取有界 stderr 行，严格解析无重复键的单个 JSON 对象，核对声称的来源／核心／运行与角色及活动运行，确保 EOS 子进程只指向已准入 EOS 节点、EGIA 子进程只指向已准入 EGIA 节点，并执行事件码专属的安全字段规则。所选 EOS runtime 可声明 `source:eos`，其 HTTP bridge 可声明 `source:bridge`，其 config server 可声明 `source:config`；所选 EGIA 子进程可声明 `source:egia`。原始 MuJoCo 诊断留在 EOS 归属目录；子进程不能冒充其他角色。编译进 IsOS 的 `event_policy` 表按审核后的 P06 归属契约固定每个角色的精确事件码、类型化字段、级别与可抑制类别，包括仅 EOS runtime 可发的 `eos_events_suppressed`；共享的 `events_suppressed` 只由 IsOS 发出。即使来源／核心／运行语法都正确，未知子进程事件码或子进程声称 IsOS 抑制码也被拒绝。EGIA 事件码已按经审核的 16 项归属策略夹具固定；未列出的事件码保持关闭。普通 stdout 和非 JSON stderr 不进入 IsOS 共享流。错误或超长行被丢弃且不缓存剩余部分；下一条有效记录仍可成为首个就绪／失败事件。收集器保留有效的子进程时间、级别、事件码、节点、关联及类型化字段，并用自身子进程句柄写入 PID。EOS 和 EGIA 在共享流之外持有完整诊断。

每条所选子进程管道由有界读取器接收，合格事件在共享轮转锁下直接写入。没有中间事件队列：可抑制的重复诊断在最多 256 键的表中计数，首次生命周期、失败、就绪与终结事件直接写入。停止和子进程回收独立于收集线程。相同序列化记录写入分类文件与统一文件；写入失败只增加 `log_write_failed`，不递归写日志。分类文件达到数量上限时仍保留统一副本。私有原始子进程 stdout/stderr 也轮转。原始行（含换行）至多 8,064 字节；加入观察到的 PID 后的存储行至多 8,192 字节。拒绝行增加 `log_ingest_rejected` 及有界原因计数。超长原始行丢弃至换行后继续解析下一事件。生产者只缩短已脱敏消息。

只在同一 `(run_id,source,node_path,event_code)` 键内去重允许抑制的非关键重复诊断。首条写入，重复项计数，摘要不快于每 60 秒一次或在终结时发出。首条警告以及所有状态变化、就绪、失败与终结事件均通过。P07 持有轮转后的首事件查找及游标；实时健康以 status 为准。

## 验证切片

### R04 所选技能事件准入设计

本设计基于 IsOS `df481774` 与 EOS SkillService 精确提交 `192fbfa6fbed1e9b6a20dd8dc95501625b02d081`；设计基线中的收集器拒绝两个技能码。此 EOS 提交在 JSONL 发布前将未回收的 guest 成功结果转为失败，因此下述 completed／succeeded 配对仍然精确。[归属接口](../interfaces/events_cn.md#r04-原生技能子进程事件实现复核)与[冻结夹具](../../../tests/fixtures/r04_skill_events/README_cn.md)规定实现边界。只扩展编译进 IsOS 的 `eos` runtime 事件码分支与专用字段校验器。保留 P06 角色表、顶层解析、活动运行核对、已准入节点核对、观察到的 PID 注入、旧套接字规范化、大小界限以及 EGIA／bridge／config 策略。图或子进程描述符都是数据，不能成为事件策略来源。

两个日志写入之前先校验完整的事件码专属字段：精确字段集、阶段／级别／原因配对及固定消息。在 Supervisor 边界把 `package_id` 与 `package_version` 核对为已准入 P04 generation 中的一项所选 P01 技能 pin；空选择不能发出 `eos_skill_execution`。所选技能版本超过 EOS 96／64 字节限制或版本语法无效时，候选部署预检必须在停止旧部署**之前**拒绝；该预检仍属于另一个 IsOS 实现切片。`worker_reaped:true` 不表示物理停止已确认。即使随后出现 `eos_stopped`，未确认停止事件仍是失败标记。

回放门禁使用上述 SHA 的 EOS 二进制和带 build metadata 包版本的所选 C guest，经真实 IsOS 所选子进程管道验证 queued、running、completed、failed 在分类及统一日志各出现一次且 PID 来自 Supervisor，再注入所有拒绝夹具并确认两种日志均不写入。依次验证 P07 首事件前、首条已接纳技能失败后、保留备份跨轮转、游标过期、收集／写入缺口及 Stop 后的当前运行页面。来源／级别／关联筛选不能改变固定首事件；之后同码事件仍是独立行；实时状态以 status 为准。完整命令及输出保存到忽略的 `logs/test/<run-id>/`，代码切片完成后请求独立实现审查。本文档及夹具本身不完成 R04 或 EOS R06 物理停止顺序门禁。

**设计证据（2026-11-02）。** `python3 tests/fixtures/r04_skill_events/check_cases.py` 通过 30 个子进程案例和十个游标场景。现有 `python3 tests/fixtures/jsonl_events/check_cases.py`（37 案例）与 `python3 tests/fixtures/log_cursors/check_cases.py`（22 个线路案例、十二个状态场景）通过。`python3 -m py_compile tests/fixtures/r04_skill_events/check_cases.py`、14 个修改／新增 Markdown 文件的本地链接检查、`/home/ashex/.cargo/bin/cargo fmt --all -- --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings`、`/home/ashex/.cargo/bin/cargo test --no-fail-fast`（207 单元、七 CLI、一个移除命令与九边界测试）及 `git diff --check` 均通过。完整输出在忽略的 `logs/test/20261102-r04-skill-events-design/`。没有改动 IsOS 运行时代码。独立设计审查与真实所选子进程回放仍待完成。

**R04 IsOS 实现证据（2026-11-02）。** 所选 EOS 收集器仅准入 `eos_skill_execution` 和 `eos_skill_stop_unconfirmed`，校验固定消息、完整字段、阶段／原因／级别、worker 类型事实、空节点、`skill_` 关联和所选 P04 技能 pin，并保留 P06 角色及 PID 边界。Rust 测试把全部 30 个子进程案例送入解析器与存储记录校验器；另以实际启动的桩子进程 stderr 验证准入和 P07 日志存储。游标测试覆盖待定／首次失败／停止失败锚点、排除筛选、拒绝缺口、同码后续位置、保留及过期轮转、已知计数写入缺口和最后运行身份。该桩不是 EOS SkillService 生产者；EOS `192fbfa6` 加所选 C guest 的真实回放以及 HTTP／CLI 当前视图仍是跨归属门禁。所选 pin 预检、EOS worker 和 R06 物理停止证据也尚待完成。`cargo fmt --all -- --check`、严格 Clippy、完整 `cargo test --no-fail-fast`（210 个单元、七个 CLI、一个已删除命令和九个边界测试）、`cargo build --all-targets` 及 R04／P06／P07 夹具检查均通过。完整输出和命令保存在忽略的 `logs/test/20261102-r04-skill-events-implementation/`；独立实现复核待完成。

**R04 实现复核修正（2026-11-02）。** 当前所选 P01／EOS 生产者语法允许 `0..1`；收集器只在软件包 ID／版本与所选精确配对一致时接纳空点段。新增正例使 R04 子进程矩阵增至 31 个案例。有效但未选择的技能 pin 现在计入 `log_ingest_rejected_by_reason.skill_pin_mismatch`，不得写入两个共享文件；`node_owner` 仍专用于节点归属故障。更严格的所选技能预停止语法门禁属于独立待完成切片。新增桩子进程回归测试同时检查所选接纳和不匹配计数。`cargo fmt --all -- --check`、`cargo check --all-targets`、严格 Clippy、完整测试（211 个单元、七个 CLI、一个已删除命令、九个边界）、构建、R04／P06／P07 夹具、链接及差异检查均通过。完整命令与输出保存在忽略的 `logs/test/20261102-r04-skill-events-review-fixes/`；EOS SkillService／C guest 与 HTTP／CLI 回放仍待完成。

[冻结夹具](../../../tests/fixtures/jsonl_events/README_cn.md)覆盖管理器会话失败、包来源、运行身份、所选子进程角色绑定、错误及超长行、不安全载荷拒绝、抑制、轮转和写入失败下停止。单元测试使用 stub 子进程管道及所选进程边界测试；合并后的真实 EOS runtime 二进制通过收集器回放启动失败路径。EGIA 经审核的 16 个事件码与归属夹具核对；合并后的 EGIA 启动器作为所选受管子进程回放，其接收事件在分类与统一日志间对比。完整输出保存于忽略 Git 的 `logs/test/<run-id>/`。

2026-11-01 在 `7898735` 加本文档分支上完成设计校验：`python3 tests/fixtures/jsonl_events/check_cases.py` 检查 30 个冻结案例与原始／存储字节边界；12 个修改后的 Markdown 文件相对链接、JSON 解析及 `git diff --check` 通过。仓库检查 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test` 通过。完整输出位于忽略 Git 的 `logs/test/p06-design-20261101/`（`docs-gates.log`、`cargo-fmt.log`、`cargo-clippy.log`、`cargo-test.log`）。这些检查不能代替上述 P06 运行时和边界测试。

2026-11-01 审查修正校验：37 案例检查器新增旧套接字归属冒充、未知子进程事件码、跨归属节点及抑制码角色检查。链接、JSON、Python 编译、格式化、Clippy 和 Cargo 测试输出位于忽略 Git 的 `logs/test/p06-design-rereview-20261101/`。先前设计运行只有 30 案例，其日志仍保留作为原始基线。

2026-11-01 实现校验，基于 IsOS `16d189f`：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`EOS_P06_RUNTIME_BIN=/home/ashex/projects/Island-eos/target/debug/aether-runtime EGIA_P06_LAUNCHER=/home/ashex/projects/Island-egia/bin/egia EGIA_P06_PYTHON=/home/ashex/projects/Island-egia/.venv/bin/python cargo test --no-fail-fast`、`cargo build --all-targets`、`python3 tests/fixtures/jsonl_events/check_cases.py`、`python3 tests/fixtures/reader_features/check_docs.py` 和 `git diff --check` 均通过。Cargo 测试为 150 个单元、5 个 CLI、1 个已移除命令及 9 个边界测试；夹具检查为 37 案例及两个字节边界。EOS runtime 二进制由已合并 EOS P06 runtime-main `f22d6e7c` 构建；EGIA 启动器和 Python 来自已合并 EGIA P06 master `a03981f`。完整命令输出位于忽略 Git 的 `logs/test/20261101-p06-jsonl-events-implementation/`（`fmt.log`、`clippy.log`、`tests.log`、`build.log`、`fixture.log`、`docs.log`、`diff.log`）。这包括真实 EOS 启动失败与真实 EGIA 受管启动器收集器回放；独立实现审查仍待完成。

2026-11-01 在 `6f643bc` 后修复实现审查问题：上述完整命令再次通过，输出保存在忽略 Git 的 `logs/test/20261101-p06-jsonl-events-review-fixes/`。Cargo 测试为 154 个单元、5 个 CLI、1 个已移除命令及 9 个边界测试。新增回归测试覆盖同一包／预检请求关联、首次 Start 失败后以新运行成功重试、旧子进程在停止事件收集器被刻意阻塞时已回收、日志存储初始化失败计数，以及含换行诊断路径不能吞掉固定预检失败事件。真实 EOS 与 EGIA 所选进程回放测试也通过。
