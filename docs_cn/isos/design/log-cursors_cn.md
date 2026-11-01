# P07 当前运行日志游标设计

[English](../../../docs/isos/design/log-cursors.md) · [归属接口](../interfaces/log-cursors_cn.md) · [P06 事件设计](events_cn.md) · [固定样例](../../../tests/fixtures/log_cursors/README_cn.md)

**目标设计——尚未实现。** [归属接口](../interfaces/log-cursors_cn.md)固定 API。P07 在 P06 事件准入后扩展 `src/runtime/log_store.rs::read_filtered`、`src/app/http.rs::logs`、`src/app/cli/mod.rs::Logs`、`src/app/entry.rs` 和管理器状态。不改变旧版 inode／offset 游标路径。IsosUI 使用显式 `view=current`，仍以 `process-status` 为实时状态权威。

## 状态与追加顺序

现有 `LogStore` 通过重命名轮转每个来源及 `events.log`，只从最新文件读取，不能跨保留备份继续，也不能辨别旧运行。在同一部署日志目录新增私有 `cursor-state.json`，保存 schema 版本、部署身份、当前管理器会话、最后准入的运行 ID、两个固定的脱敏锚点、文件段代际和有界摄取／写入丢失计数。状态文件和临时替换文件为 `0600`，父目录为 `0700`；写入、fsync、rename 后 fsync 父目录。状态无效、符号链接或身份不符时封闭失败，返回 `unavailable` 锚点和有类型缺口，同时保留状态权威。状态文件不存可执行路径、原始行、秘密或图内容。

P06 目前从 Validate 和 Apply 均发出 `deployment_preflight_failed`。向 P07 锚点捕获传递私有调用点操作标记：只允许 Apply 调用点；有类型的软件包故障无论在哪个预检阶段都合格。标记不能从 `message` 或 `event_code` 推断，也无需新增 P06 线路字段。当前管理器会话的首条此类 `error` 在公共错误返回前，先持久化其已脱敏 P06 事件副本。所选子进程收集器先验证角色、运行和事件码，才可选择运行锚点；该准入运行的首条归属方就绪码或已接纳 `error` 在收集确认前复制。来源不可信的子进程事件或 schema-1 兼容 socket 均不合格。锚点是保留记录，不是第二个生产者或伪造事件。持久化失败时设置有界丢失计数，报告 `log_anchor_unavailable`；普通统一／来源追加可独立成功或失败。新运行准入且范围 ID 持久发布前，保留旧运行锚点；Restart 预检失败保留旧运行。候选和恢复运行使用 P06 独立 ID。

范围选择器读取最后准入运行身份，不读取 PID 或 `active_revision` 以决定范围。该值跨停止、失败和管理器重启保留，只在新运行准入时前进。首次准入前使用当前管理器会话。新的管理器会话会更换管理器范围，旧管理器游标收到 `log_run_changed`。在运行尚无就绪／故障时，返回待定锚点、空 `lines` 和绑定该运行的轮询 token；锚点到达后该 token 可继续。状态／日志发布撕裂时，只能扫描保留段中同一运行的已接纳事件并核对 P06 丢失计数；不能选别的运行或从进程状态合成事件。无法恢复则返回 `unavailable` 和缺口。

## 读取路径

对于 `view=current`，先选择范围和固定锚点，再把有界 `lc1.` token 解码并认证到部署、范围类别／ID 及规范化筛选器。格式错误或改变筛选器返回有类型 HTTP 400，不回显 token。有效的旧范围 token 返回 HTTP 200、`log_run_changed`、新锚点及新游标。根据段代际和偏移，在 `events.log.<N>` 与 `events.log` 中由旧到新读取，每次最多扫描 64 KiB 原始日志字节、返回最多 200 条匹配记录。保留段跨轮转时 `rotated:true`，无缺口。过期段从锚点之后最早保留记录重定位，`rotated:true` 并报告 `log_cursor_expired`；除非持久序号可证明精确数量，否则数量为 null。不能静默从最新文件开始。筛选后空页也推进游标。

逐行解析为 P06 存储事件，先核对 `run_id` 与范围，再对锚点之后的行取来源、最低级别、节点和关联四种筛选条件的交集。锚点在行外返回，不受筛选影响。游标来自最后扫描位置，不是最后显示位置。每个部署使用独立私有 `cursor-key` 文件（`0600`）保存持久 MAC 密钥，认证版本、部署、范围、筛选、段和偏移；其编码不是公开格式。密钥轮换或不可读使旧 token 以 `log_cursor_invalid` 失效；若无法证明保留连续性，还报告收集缺口。

`log_collector_gap` 来自 P06 有界拒绝／丢弃／写入故障计数，不能仅从时间跳跃或 inode 变化推断。持久记录精确数量时返回非负数，否则为 null。诊断固定顺序：运行变更、段过期、收集缺口、锚点不可用。子进程在就绪／故障事件前终止时，状态仍是权威。UI 在筛选行前显示固定锚点、显示缺口、换范围时清空当前行，并持续显示 Stop 与实时状态。Stop 后最后运行的终结事件仍可见；不能因进程消失切回管理器会话。

## 验证

[P07 固定样例](../../../tests/fixtures/log_cursors/README_cn.md)固定响应键、管理器／运行锚点、筛选、分页进度、跨备份、过期游标、换运行、收集中断和终结保留。单元测试使用模拟文件段代际、有界读取器和模拟时钟。边界测试使用真实 `LogStore`、HTTP 路由、CLI 与所选 P06 子进程流。注入 rename／fsync 失败和管理器重启，确认状态仍权威且不伪造就绪事件。完整 IsOS format、Clippy、test、build 和 IsosUI 边界输出保存于忽略的 `logs/test/<run-id>/`。本设计提交仅提供契约证据。

**设计证据（2026-11-01）。** `python3 tests/fixtures/log_cursors/check_cases.py` 的 18 个案例通过；`python3 -m json.tool tests/fixtures/log_cursors/cases.json`、`python3 -m py_compile tests/fixtures/log_cursors/check_cases.py`、对 12 个变更页面的本地 Markdown 链接检查及 `git diff --check` 均通过。完整命令输出保存在被忽略的 `logs/test/20261101-p07-log-cursors-design/`。本设计切片没有改动运行时代码或边界测试。 使用 `/home/ashex/.cargo/bin/cargo` 运行的 `cargo fmt --all -- --check` 通过。
