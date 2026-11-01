# P07 当前运行日志游标设计

[English](../../../docs/isos/design/log-cursors.md) · [归属接口](../interfaces/log-cursors_cn.md) · [P06 事件设计](events_cn.md) · [固定样例](../../../tests/fixtures/log_cursors/README_cn.md)

**P07 已实现。** [归属接口](../interfaces/log-cursors_cn.md)固定 API。P07 在 P06 事件准入后扩展 `src/runtime/log_store.rs::read_filtered`、`src/app/http.rs::logs`、`src/app/cli/mod.rs::Logs`、`src/app/entry.rs` 和管理器状态。不改变旧版 inode／offset 游标路径。IsosUI 使用显式 `view=current`，仍以 `process-status` 为实时状态权威。

## 状态与追加顺序

现有 `LogStore` 通过重命名轮转每个来源及 `events.log`，只从最新文件读取，不能跨保留备份继续，也不能辨别旧运行。在同一部署日志目录新增私有 `cursor-state.json`，保存 schema 版本、部署身份、当前管理器会话、最后准入的运行 ID、两个固定的脱敏锚点、文件段代际和有界摄取／写入丢失计数。状态文件和临时替换文件为 `0600`，父目录为 `0700`；写入、fsync、rename 后 fsync 父目录。新部署初始化在 P07 提供管理器范围前持久记录 `never_run`；现有部署缺失或损坏状态文件不能视为从未运行。状态无效、符号链接或身份不符时封闭失败。保留的 `deployment_starting` 事件无法证明之后没有新运行准入但启动日志追加失败。状态记录无效时不能从 `events.log` 恢复最后运行；此时设置 `process-status.run_id_state:unavailable`，并从 `view=current` 返回 HTTP 503 `log_scope_unavailable`、路径 `/scope`；绝不伪造管理器范围。该状态不是子进程权限来源。状态文件不存可执行路径、原始行、秘密或图内容。

使用独立且稳定的 `scope-key`（`0600`）认证完整私有状态；首次发布状态前创建该密钥并 fsync 父目录。每次替换状态均在解除子进程准入屏障前更新范围封印。现有状态的密钥丢失后不得重新创建：证明缺失或不匹配使 `run_id_state` 不可用，现有无效状态不能进入 P06 升级路径。仅当状态文件和范围密钥均不存在时才允许该路径。该证明独立于可轮换的 `cursor-key`，因此游标密钥改变会使旧页 token 失效，也可能失去锚点证明，但不能改变已证明的最后运行 ID。

P06 目前从 Validate 和 Apply 均发出 `deployment_preflight_failed`。向 P07 锚点捕获传递私有调用点操作标记：只允许 Apply 调用点；有类型的软件包故障无论在哪个预检阶段都合格。标记不能从 `message` 或 `event_code` 推断，也无需新增 P06 线路字段。当前管理器会话的首条此类 `error` 在公共错误返回前，先持久化其已脱敏 P06 事件副本。所选子进程收集器先验证角色、运行和事件码，才可选择运行锚点；该准入运行的首条归属方就绪码或已接纳 `error` 在收集确认前复制。来源不可信的子进程事件或 schema-1 兼容 socket 均不合格。锚点是保留记录，不是第二个生产者或伪造事件。持久化失败时设置有界丢失计数，报告 `log_anchor_unavailable`；普通统一／来源追加可独立成功或失败。新运行准入且范围 ID 持久发布前，保留旧运行锚点；Restart 预检失败保留旧运行。准入在全部预检和安全替换停止／回收之后设置明确屏障：分配候选运行 ID，原子写入新最后运行 ID 与 `pending` 锚点状态，fsync 文件和父目录，之后才启动首个子进程。发布失败时不发出启动事件，也不启动子进程；以 `/run_id` 处的 `log_scope_publish_failed` 返回，若写入在 rename 前失败，保留旧持久范围。rename 成功但父目录 fsync 失败时同样返回有类型错误并禁止启动；恢复时若新记录幸存，则视为已准入并显示失败状态；没有已接纳故障事件时将锚点标记 `unavailable` 并报告 `log_anchor_unavailable`，绝不悄悄回退或永久待定。成功发布后但启动前崩溃也留下已准入运行，不伪造就绪事件；恢复独立报告失败和状态；若没有已接纳故障事件，锚点同样为 `unavailable`。候选和恢复运行使用 P06 独立 ID。

范围选择器读取最后准入运行身份，不读取 PID 或 `active_revision` 以决定范围。该值跨停止、失败和管理器重启保留，只在新运行准入时前进。首次准入前使用当前管理器会话。新的管理器会话会更换管理器范围，旧管理器游标收到 `log_run_changed`。在运行尚无就绪／故障时，返回待定锚点、空 `lines` 和绑定该运行的轮询 token；锚点到达后该 token 可继续。身份记录撕裂或无效时封闭失败；保留的 P06 日志不能排除之后已有准入运行但其启动追加失败。候选行和固定锚点仍须通过完整 P06 信封与已接纳位置检查；不能选别的运行或从进程状态合成事件。有效的最后运行记录即使原锚点事件已轮转，也继续决定运行身份；恢复固定副本，保持停止／失败范围。身份已知但锚点无法恢复时返回 `anchor.state:unavailable` 和 `log_anchor_unavailable`。身份无法证明时返回 HTTP 503 `log_scope_unavailable`，而非 HTTP 200 管理器页；`process-status` 保留实时字段并标记 `run_id_state:unavailable`。

## 读取路径

对于 `view=current`，先选择范围和固定锚点，再把有界 `lc1.` token 解码并认证到部署、范围类别／ID 及规范化筛选器。格式错误或改变筛选器返回有类型 HTTP 400，不回显 token。无效 `node_path` 与 `correlation_id` 查询值分别在 `/query/node_path` 返回 `log_node_path_invalid`、在 `/query/correlation_id` 返回 `log_correlation_id_invalid`，先于游标匹配。范围不可用时在 `/scope` 返回 HTTP 503 `log_scope_unavailable`。有效的旧范围 token 返回 HTTP 200、`log_run_changed`、新锚点及新游标。根据段代际和偏移，在 `events.log.<N>` 与 `events.log` 中由旧到新读取，每次最多扫描 64 KiB 原始日志字节、返回最多 200 条匹配记录。保留段跨轮转时 `rotated:true`，无缺口。过期段从锚点之后最早保留记录重定位，`rotated:true` 并报告 `log_cursor_expired`；除非持久序号可证明精确数量，否则数量为 null。不能静默从最新文件开始。筛选后空页也推进游标。

逐行解析为 P06 存储事件，先核对 `run_id` 与范围，再对锚点之后的行取来源、最低级别、节点和关联四种筛选条件的交集。锚点在行外返回，不受筛选影响。游标来自最后扫描位置，不是最后显示位置。固定脱敏副本旁保存锚点已接纳的文件段代际／偏移，并按此位置比较各候选行；之后同一 `event_code` 的事件是独立行，不得丢弃。每个部署使用独立私有 `cursor-key` 文件（`0600`）保存持久 MAC 密钥，认证版本、部署、范围、筛选、段和偏移；其编码不是公开格式。密钥轮换或不可读使旧 token 以 `log_cursor_invalid` 失效，同时保留独立有效的持久运行 ID。无法证明旧锚点封印时，新页面返回同一运行，但标记锚点不可用、收集缺口数量未知。不可用的私有密钥将被原子替换，以便签发新 token。

`log_collector_gap` 来自 P06 有界拒绝／丢弃／写入故障计数，不能仅从时间跳跃或 inode 变化推断。持久记录精确数量时返回非负数，否则为 null。诊断固定顺序：运行变更、段过期、收集缺口、锚点不可用。子进程在就绪／故障事件前终止时，状态仍是权威。UI 在筛选行前显示固定锚点、显示缺口、换范围时清空当前行，并持续显示 Stop 与实时状态。Stop 后最后运行的终结事件仍可见；不能因进程消失切回管理器会话。

## 验证

### R04 技能事件游标回放门禁

精确 EOS 技能事件码通过 P06 所选子进程准入后，P07 不增加范围类别或 token 格式。以所选 `eos_skill_execution` 失败作为运行中首条合格事件回放，证明封印副本在来源、级别、关联筛选前作为 `anchor.event` 返回。只有 queued／running／completed 时锚点仍待定。被拒绝或错误角色的技能记录不能成为锚点或分页行。其后同码事件具有不同接纳位置，必须在锚点后的 `lines` 中返回。先在保留游标段的情况下轮转 `events.log`，再使该段过期；前者连续分页并有 `rotated:true`，后者报告 `log_cursor_expired` 且保留原封印锚点。注入收集／写入缺口并保持真实计数或 null。Stop 后技能停止故障仍属于最后准入运行。[R04 夹具](../../../tests/fixtures/r04_skill_events/README_cn.md)规定案例；实现必须使用 EOS `192fbfa6fbed1e9b6a20dd8dc95501625b02d081` 经真实 `LogStore` 与 HTTP／CLI 当前视图验证。

[P07 固定样例](../../../tests/fixtures/log_cursors/README_cn.md)固定响应键、管理器／运行锚点、筛选、分页进度、跨备份、过期游标、换运行、收集中断和终结保留。单元测试使用临时保留文件段与有界读取。边界测试使用真实 `LogStore`、HTTP 路由、管理器子进程屏障以及所选 P06 EOS／EGIA 子进程流。发布与锚点故障、状态损坏及管理器会话变化均按关闭原则处理。完整 IsOS format、Clippy、test 和 build 输出位于忽略的 `logs/test/<run-id>/`；IsosUI 呈现边界属于其独立切片。此前的设计提交先于实现提供契约证据。

**设计证据（2026-11-01）。** `python3 tests/fixtures/log_cursors/check_cases.py` 的 21 个线路案例和 9 个状态场景通过；`python3 -m json.tool tests/fixtures/log_cursors/cases.json`、`python3 -m py_compile tests/fixtures/log_cursors/check_cases.py`、对 12 个变更页面的本地 Markdown 链接检查及 `git diff --check` 均通过。完整命令输出保存在被忽略的 `logs/test/20261101-p07-log-cursors-design/`。本设计切片没有改动运行时代码或边界测试。 使用 `/home/ashex/.cargo/bin/cargo` 运行的 `cargo fmt --all -- --check` 通过。

**设计修订证据（2026-11-01）。** `python3 tests/fixtures/log_cursors/check_cases.py` 的 21 个线路案例和 7 个可执行状态场景通过。两个样例 JSON 均可解析；Python 编译、12 个页面的本地链接、`cargo fmt --all -- --check` 和 `git diff --check` 均通过。输出保存在被忽略的 `logs/test/20261101-p07-log-cursors-design-r2/`。

**样例参考检查器修订证据（2026-11-01）。** 参考检查器按已接纳位置比较，保留之后的同码事件，并从完整双运行历史恢复最新运行。`python3 tests/fixtures/log_cursors/check_cases.py` 的 21 个线路案例及 9 个状态场景通过；两个 JSON 解析、Python 编译、12 页链接检查、`cargo fmt --all -- --check` 和 `git diff --check` 均通过。输出保存在被忽略的 `logs/test/20261101-p07-log-cursors-design-r3/`。

**实现证据（2026-11-01）。** 本 IsOS 工作树的 `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --no-fail-fast` 与 `cargo build --all-targets` 均通过。测试通过 `EOS_P06_RUNTIME_BIN=/home/ashex/projects/Island-eos/target/debug/aether-runtime`、`EGIA_P06_LAUNCHER=/home/ashex/projects/Island-egia/bin/egia` 和 `EGIA_P06_PYTHON=/home/ashex/projects/Island-egia/.venv/bin/python` 选用真实归属方生产者，并将其故障事件流交给 P07 范围读取器。测试覆盖子进程生成前的发布失败、不确定发布、双运行恢复、锚点写入失败、绑定筛选条件的游标、有界扫描、保留与过期轮转以及有类型 HTTP 错误。`python3 tests/fixtures/log_cursors/check_cases.py` 的 21 个线路案例和 9 个状态场景通过；两个样例 JSON 解析、Python 编译、六个变更页面链接检查及 `git diff --check` 均通过。完整输出位于被忽略的 `logs/test/20261101-p07-log-cursors-implementation/`。

**实现审查修订（2026-11-01）。** 恢复现要求有效的持久范围状态；保留的启动事件无法证明不存在之后已接纳的运行，因此上文样例参考检查器关于乐观恢复的陈述已被取代。管理器会话意向记录可阻止状态替换失败后重新开放旧范围。固定锚点须具有完整 P06 信封、合格的来源和代码、匹配的范围、私有校验封印，并在原始段仍保留时核对已接纳记录的位置。候选行若 JSON 损坏或键重复，会被跳过并报告收集缺口。真实 EOS／EGIA 生产者重放及完整门禁通过：175 个单元测试、6 个 CLI 测试、1 个已移除命令测试和 9 个边测试；样例检查器通过 21 个线路案例和 10 个状态场景。可重现命令及完整输出保存在被忽略的 `logs/test/20261101-p07-log-cursors-review-fixes/`。

**游标密钥审查修订（2026-11-01）。** 游标密钥被替换、删除或不可读时，独立有效的范围记录仍保留最后已准入运行。旧 token 返回 `log_cursor_invalid`；新页面显示同一运行、无法证明的锚点及数量未知的收集缺口。针对三种密钥故障的测试重新打开日志存储。格式、Clippy、完整测试（176 个单元、6 个 CLI、1 个已移除命令及 9 个边测试）、构建、JSON／Python 检查、30 个本地链接及差异检查均通过；样例检查器通过 22 个线路案例和 11 个状态场景。完整输出及可重现命令保存在被忽略的 `logs/test/20261101-p07-key-rotation-fix/`。

**范围证明审查修订（2026-11-01）。** 稳定的归属方专用 `scope-key` 现独立于游标密钥认证完整持久状态。将已保存的 `r812` 运行 ID 和锚点事件改为伪造的 `r999` 后，原范围封印无法认证；状态和当前日志页封闭失败，新准入也不能将无效状态当作 P06 升级。测试还覆盖范围密钥丢失、范围密钥仍在但状态文件丢失、真实 P06 升级路径，以及三种游标密钥轮换情形。格式、Clippy、完整测试（179 个单元、6 个 CLI、1 个已移除命令及 9 个边测试）、构建、JSON／Python 检查与差异检查均通过；样例检查器通过 22 个线路案例和 12 个状态场景。完整输出和可重现命令保存在被忽略的 `logs/test/20261101-p07-scope-proof-fix/`。
