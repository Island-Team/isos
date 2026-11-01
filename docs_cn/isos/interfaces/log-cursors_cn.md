# 当前运行日志游标 — P07

[English](../../../docs/isos/interfaces/log-cursors.md) · [P06 事件](events_cn.md) · [本地 API](local-api_cn.md) · [设计](../design/log-cursors_cn.md) · [首次使用](../usage/get-started_cn.md) · [P07 任务](../../../../Island-eos/docs/island/implementation-todo.md)

**P07 已实现。** IsOS 负责每个部署的当前日志范围、保留的首条事件、游标、轮转和缺口诊断。IsosUI 呈现归属方状态。P06 脱敏事件信封和 IsOS 进程状态分别是事件内容与实时健康的权威。本扩展不改变未带 `view=current` 的旧版 `GET /api/isos/v1/logs`、schema-1 HTTP 信封或未带 `--current` 的 `isos logs` 尾部输出。

## 端点和查询

`GET /api/isos/v1/logs?view=current&source=all&level=info&limit=100` 在现有 `{schema_version:1,ok:true,data:{...}}` 信封中返回 HTTP 200。归属方专用 loopback Host/Origin 检查不变。`view=current` 是唯一新增视图值。可选查询字段如下：

| 字段 | 类型／单位 | 用途、默认、验证和兼容性 |
| --- | --- | --- |
| `view` | 固定值 `current`／— | 启用 P07 范围和游标规则；省略时维持现有文件游标与响应。其他值以 HTTP 400 `log_view_invalid` 拒绝。 |
| `source` | 来源 ID 或 `all`／— | 默认 `all`；沿用现有路由校验过的来源集合。在选择锚点后应用。 |
| `level` | `trace`、`debug`、`info`、`warn`、`error`／最低级别 | 默认 `info`；在锚点后应用，不筛选固定锚点。 |
| `node_path` | 规范保存图节点 JSON Pointer／— | 可选精确节点筛选，最多 512 UTF-8 字节，须符合 P06 规范的保存图节点指针语法；禁止成员指针或本地路径。节点路径为 null 的事件不匹配。无效值以 HTTP 400 `log_node_path_invalid` 和 `/query/node_path` 拒绝。 |
| `correlation_id` | ASCII ID／— | 可选精确关联筛选，1–64 个 ASCII 字母、数字、`_` 或 `-`；null 不匹配。无效值以 HTTP 400 `log_correlation_id_invalid` 和 `/query/correlation_id` 拒绝。 |
| `limit` | 整数／记录 | 默认 100，范围 1–200。每次最多扫描 64 KiB 保留日志字节；筛选后的空页仍可能推进游标。 |
| `cursor` | 不透明字符串／位置 | 可选，最多 512 ASCII 字节。P07 token 以 `lc1.` 开头，但客户端不得解析或依赖其内容。token 认证部署、范围类别／ID、规范化筛选器、文件段代际和位置。篡改、旧版 inode 游标、错误部署或改变筛选器以 HTTP 400 `log_cursor_invalid` 或 `log_cursor_filter_mismatch` 拒绝，不回显原 token。 |

`GET /api/isos/v1/process-status` 新增可为 null 的 `run_id` 和 `run_id_state`（`none`、`known` 或 `unavailable`）。`run_id_state:known` 时，`run_id` 表示**最后一个已准入部署运行**；只有 `none` 或 `unavailable` 时可为 null，且两者必须可区分。停止、失败、PID 丢失及 `active_revision:null` 时仍保留，仅在新运行准入时改变。`state`、`processes`、`last_error` 和活跃修订仍是实时状态权威；日志页不能使其就绪。仅当持久 `never_run` 标记证明该部署从未准入运行时，管理器会话 ID 才作为 `scope.run_id` 返回。全新部署在提供 P07 读取前 fsync 此标记；现有部署缺少标记表示身份不可用，不能当作首次使用证据。IsOS 将最后运行身份与归属日志游标状态一同持久化，管理器重启不能悄悄将已停止部署切回管理器范围。如果无法恢复该身份，`process-status` 仍返回实时字段及 `run_id:null,run_id_state:"unavailable"`；`view=current` 以 HTTP 503 `log_scope_unavailable` 和 `/scope` 失败，绝不伪造管理器范围。

## 响应

`data` 具有以下精确 P07 字段；`anchor.event` 和 `lines` 内仍是原有 P06 事件对象：

| 字段 | 类型／单位 | 含义 |
| --- | --- | --- |
| `scope` | `{kind,run_id,active_revision}` 对象／— | `kind` 为 `manager_session` 或 `deployment_run`；`run_id` 为所选 `m...` 管理器会话或最后准入的 `r...` 部署运行 ID（1–64 ASCII 字节）；`active_revision` 为活跃图摘要或 null，仅供显示。调用方不能选择运行 ID。 |
| `anchor` | `{kind,state,event}` 对象／— | 管理器范围 `kind:first_prelaunch_failure`，运行范围 `kind:first_ready_or_failure`。`state` 为 `pending`、`found` 或 `unavailable`。仅在找到时，`event` 是完整 P06 存储事件；否则为 null。它位于 `lines` 之外且不受显示筛选。 |
| `lines` | P06 存储事件数组／记录 | 最多 `limit` 条同范围且**严格晚于**锚点的事件；所有显示筛选器取交集。锚点不重复出现；之后已接纳的同一 `event_code` 仍须出现。用内部已接纳文件段／偏移位置比较，不按事件码或消息相等比较。首个锚点待定时为空。 |
| `next_cursor` | 不透明字符串／位置 | 总是存在，包括空页和待定页。使用同一规范筛选器继续轮询。新运行返回新 token。 |
| `rotated` | 布尔值／— | 本页跨越保留文件段边界时为 true；保留轮转本身不代表丢失。保留给现有 UI 兼容。 |
| `diagnostics` | `{code,count}` 数组／缺口 | 零个或多个有类型、有界诊断。已知数量用非负整数，未知用 null。绝不包含原始行、路径或 token。 |

诊断码为 `log_run_changed`（旧运行游标重定位到当前范围）、`log_cursor_expired`（文件段超出备份保留范围）、`log_collector_gap`（P06 摄取／写入丢失计数增加）、`log_anchor_unavailable`（固定首条事件无法持久化或恢复）。多项同时出现时按此顺序返回。运行变更或游标过期返回 HTTP 200 和新的范围／锚点／游标，不静默返回空页；格式错误或筛选器不匹配的 token 返回 HTTP 400。`rotated:true` 可伴随缺口，但自身不代表缺口。未知丢失数量不能伪报为零。

P07 查询错误沿用 schema-1 信封：HTTP 400 返回 `{"schema_version":1,"ok":false,"data":{"diagnostics":[{"path":"/query/cursor","code":"log_cursor_filter_mismatch","message":"Log cursor filters changed"}]},"error":"log cursor rejected"}`。`log_cursor_invalid` 使用同一游标路径和固定安全消息；`log_view_invalid` 使用 `/query/view`；`log_node_path_invalid` 和 `log_correlation_id_invalid` 分别使用 `/query/node_path` 和 `/query/correlation_id`。无法恢复运行身份时，HTTP 503 返回 `data:{diagnostics:[{path:"/scope",code:"log_scope_unavailable",message:"Current log scope unavailable"}]}` 及 `error:"log scope unavailable"`。发布失败时，process-control 以 HTTP 500 返回 `data:{diagnostics:[{path:"/run_id",code:"log_scope_publish_failed",message:"Log scope publication failed"}]}` 与 `error:"log scope publication failed"`，且没有子进程启动。诊断不回显 token 或筛选值。

即使查询 `source=eos&level=error`，初始管理器故障页仍返回：

```json
{"schema_version":1,"ok":true,"data":{"scope":{"kind":"manager_session","run_id":"m42-abc","active_revision":null},"anchor":{"kind":"first_prelaunch_failure","state":"found","event":{"timestamp":"2026-11-01T10:00:00.000Z","level":"error","source":"packages","core":"isos","node_path":"/cores/eos/layers/process/groups/mujoco/nodes/sim","event_code":"package_load_failed","run_id":"m42-abc","correlation_id":"p3","message":"Verified package input changed; relink the exact release","fields":{"package_id":"island/mujoco-sim","package_version":"0.1.1"},"pid":42}},"lines":[],"next_cursor":"lc1.OPAQUE","rotated":false,"diagnostics":[]}}
```

锚点选择发生在来源、级别、节点和关联筛选**之前**。没有符合条件的事件时，`anchor.state:"pending"`、`anchor.event:null`、`lines:[]`；用返回的游标轮询直到就绪／故障出现。锚点存储失败时用 `unavailable` 加 `log_anchor_unavailable`，不能永远误报待定。找到锚点后首个筛选页从该记录之后开始。UI 可独立显示固定锚点，再追加 `lines`，无需按消息文本去重。

## 范围、保留和恢复

部署首次准入运行前，IsOS 选择当前管理器会话，固定有类型的软件包或 Apply 故障中的**首条** `error`；Run／Restart 预检中的软件包故障也符合条件；仅从 Apply 调用点选择 `deployment_preflight_failed`，不选择 Validate 或一般 Run／Restart 预检发出的同名事件；不按消息文本或任意子进程事件码识别。启动前故障在 API／CLI 错误返回之前带管理器会话运行 ID 和可选请求关联写入。存在准入运行后立即选择它；Stop、失败、进程退出、活跃修订清空及管理器重启后仍保留。后续准入运行原子替换它，过时游标报告 `log_run_changed`。准入必须在**首次子进程启动前**将新运行 ID 与 `pending` 锚点状态持久发布（文件及父目录 fsync）。发布失败以 `/run_id` 处的有类型 `log_scope_publish_failed` 阻止启动，不能悄悄沿用旧范围。fsync 结果不确定也阻止启动；恢复读取持久记录，将幸存的新运行视为已准入，不能因 PID 缺失回退。若因此前未启动子进程且没有已接纳故障事件，标记锚点 `unavailable` 并报告 `log_anchor_unavailable`，不永远等待。P07 不新增清除端点。较早运行中或停止后发生的管理器会话错误仍可在旧版 `source=packages|isos` 视图和状态中获取，但不重新分配当前运行范围。

对部署运行，IsOS 固定首条已接纳的就绪码（`deployment_ready`、`eos_ready`、`bridge_ready`、`config_ready` 或已审核的 `egia_ready`），**或**归属角色／IsOS 部署的首条已接纳 `error` 事件，取先到者。旧 socket 追加和原始子进程行不能成为锚点。IsOS 在确认收集前将脱敏首条事件复制到归属方专用 `0600` 锚点记录；普通 `events.log` 轮转后仍保留。符合条件的记录出现前锚点为待定，`lines` 为空。来源、级别、节点和关联筛选仅在范围／锚点选择后应用。重启／恢复尝试拥有独立运行和锚点；不能将旧运行行拼接进新页。

IsOS 按时间顺序读取当前 `events.log` 及配置的 `1..10` 个备份（每文件 `65,536..100,000,000` 字节；默认 256 KiB 和两个备份）。绑定到保留文件段的游标跨轮转继续读取且不丢失。段过期时重定位到固定锚点之后最早保留的记录，并以未知数量报告 `log_cursor_expired`；固定锚点仍可见。当前管理器会话锚点、最后运行锚点、运行 ID、段身份及有界丢失计数保存在私有 `data/logs/<deployment>/cursor-state.json`，原子替换并 fsync 父目录。每个部署最多保留这两个锚点副本和一个当前范围；它不是第二条事件流。管理器重启后先读取持久最后运行记录，再与保留日志及 P06 状态核对，不伪造缺失事件。已停止或失败的最后运行在源事件轮转后仍保有身份和固定锚点。保留的 `deployment_starting` 不能证明完整准入历史：之后的新运行可能已发布，但其启动事件追加失败。范围状态记录无效时，即使多个较早启动事件仍在，也将 `run_id_state` 标记为 `unavailable`，`view=current` 返回 `log_scope_unavailable`；不能猜测最后运行或回退到当前管理器会话。收集器不可用时保留已知丢弃计数，未知则报告 null，恢复时发 `log_collector_gap`。持久锚点必须符合完整 P06 信封、所选归属方事件码及角色策略、当前范围和带校验的已接纳位置。原文件段仍保留时还会核对该位置的事件副本；轮转移除后使用私有校验值。锚点语义无效时封闭失败。独立私有游标密钥被替换、丢失或不可读时，旧 token 返回 `log_cursor_invalid`，而有效的持久最后运行 ID 仍保持 `known`；若旧封印无法证明，新页面保留该运行，但以 `anchor.state:unavailable`、数量为 null 的 `log_collector_gap` 和 `log_anchor_unavailable` 报告。IsOS 原子替换不可用密钥以签发新 token。候选行若有畸形或重复 JSON 键、缺失信封字段或不匹配的归属方策略，将跳过并报告 `log_collector_gap`，不予显示。管理器会话变更先写入私有持久意图，再替换范围；替换失败使旧范围对各读取器不可用。日志写入或游标状态故障不能拖延致动停止／回收；状态报告故障，日志页报告缺口。

独立且稳定的归属方专用 `scope-key`（`0600`）对完整私有范围状态签名，包括最后已准入运行 ID 和锚点副本；它与可轮换的 `cursor-key` 分离。IsOS 在首次发布状态前持久创建该密钥，现有状态的密钥丢失后绝不悄悄重建。密钥缺失、范围封印变更或状态语义无效使范围不可用；只有范围封印通过后，`cursor-key` 变更才可降级锚点证明。现有无效状态不能被当作旧 P06 目录来发布新运行；P06 升级路径仅适用于状态文件和范围密钥均不存在的情形。

CLI 新增 `isos logs [source] --current --level info --lines 100` 单次读取当前范围：先打印不受筛选的锚点，再打印筛选行，并将缺口诊断写向 stderr。不带 `--current` 的现有尾部行为不变。P07 不新增图创作日志路由，也不根据事件顺序推断健康。
