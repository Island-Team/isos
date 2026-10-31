# IsOS 配置、版本与日志契约

[English](../../../docs/isos/interfaces/configuration.md)

**状态：** XDG／`ISOS_HOME` 目录、`config/isos.toml`、schema-1 本地 API、按来源轮转的日志和 `isos version check` 已实现。不可变生成快照、跨核心事件字段及受支持版本数组是 **0.x 目标**。当前字段见[本地 API](local-api_cn.md#本地运行配置)，操作顺序见[首次使用](../usage/get-started_cn.md)。

## 目录与优先级

`ISOS_HOME` 必须为绝对路径；设置后根目录为 `<base>/config`、`<base>/data`、`<base>/run`。否则使用 `${XDG_CONFIG_HOME:-~/.config}/isos`、`${XDG_DATA_HOME:-~/.local/share}/isos` 与 `${XDG_RUNTIME_DIR:-/run/user/<uid>}/isos`。IsOS 使用仅所有者可访问的目录（`0700`）和私有文件（`0600`），不把命令工作目录当作配置目录。

| 相对路径／所有者 | 类型与用途 | 写入和校验规则 |
| --- | --- | --- |
| `config/deployments/<name>.nodes.json`／IsOS | 人工编写的部署图 | 名称通过 slug 校验；修订为已保存字节的 SHA-256；只有修订检查后的 Apply 或显式迁移可写。 |
| `config/services/<id>.toml`／IsOS | 独立启动的模型类服务 | 只能通过显式服务命令启动；本次不作为图节点。 |
| `config/mode`／IsOS | `dev` 或 `product` | `isos init` 写入；无效值阻止启动。 |
| `data/generated/<name>/<revision>/eos/` 与 `egia/`／IsOS | 已校验的核心配置快照 | **目标：**按已保存修订命名的不可变目录；先写私有临时文件、fsync，再原子发布；仅写入有界 setup 适配器返回的文件。当前实现使用部署级目录。 |
| `data/packages/<namespace>/<name>/<version>/`／IsOS | 精确版本包、收据和节点类型侧车 | 安装或链接只产生收据，不激活图；Apply 与 Run 重新计算链接内容指纹。 |
| `data/logs/<name>/`／IsOS 与 EOS 子进程 | 按来源轮转的 JSONL 和统一 `events.log` | 仅所有者访问；有界游标读取；目标事件含运行 ID。 |
| `data/cache/`／IsOS | 下载和目录缓存 | **目标：**可删除；包身份依赖收据和摘要，不依赖缓存存在。 |
| `run/manager.sock`、`run/<name>-*.sock`／所属进程 | 私有管理器与核心套接字 | 检查 UID；确认旧所有者身份后才清理遗留套接字。 |

设置 `ISOS_CONFIG` 时它指定必须存在的绝对 TOML 文件，否则读取 IsOS 仓库的 `config/isos.toml`。每个字段依次采用显式环境变量、TOML、文档默认值。TOML 中的相对源码产物路径以 IsOS 仓库为基准；`ISOS_HOME`／XDG 独立选择私有状态根目录。不在图设置、生成快照、收据、日志或 UI 偏好中存放密钥。服务 `env_from` 在启动时导入命名环境变量；状态只显示名称，不显示值。缺少必需密钥时预检失败，且不回显值。

## 版本准入

| 身份 | 目标值或规则 | 兼容含义 |
| --- | --- | --- |
| 产品发行 | IsOS／EGIA／IsosUI `0.1.0`、EOS `0.2.0`、MuJoCo 包 `0.1.1` | 没有产品宣称稳定版 `1.0.0`。 |
| 新图写入格式 | `0.1.0` | 读取器也接受已有 `1.0.0` 图；原始字节不变。 |
| 新节点类型 ID | `@v0.1` | 旧 `@v1.0`／`@v1.1` 保持已注册的安全行为。 |
| HTTP 路由和 setup 封装 | `/api/isos/v1`、`schema_version:1` | 兼容身份，不是产品版本。 |
| 包发行清单 | `schema_version:4` | 分发格式；精确包版本另计。 |
| MuJoCo 套接字协议 | `2.0.0` | 已有仿真器连接身份；EOS 与包必须一致。 |

**目标算法：**不启动核心，先探测每个已启用核心的 setup `identity`；读取 `supported_graph_versions`、`supported_node_type_versions`、setup schema 与产品版本；与实际保存图和精确包贡献逐一比较。校验包收据／摘要及已编译适配器。`isos version check --json` 对每个不匹配项返回受支持列表和可执行操作。Apply 与 Run 针对当前字节和程序重复校验；包、图或程序变化使生成快照失效。不可把 `1.0.0` 图伪装为 `0.1.0` 以通过校验。当前 identity 只有 `graph_version` 与 `node_type_version` 单个字符串，数组是目标扩展。

## 目标统一 JSONL 事件

模块在首次就绪时写一条 `info`，随后只记录状态变化、警告和错误；周期健康检查只在变化时或有界汇总间隔记录。完整子进程输出由其所有者保存，IsOS 统一流只接收有界、已脱敏事件。日志写入不得阻塞执行器停止或进程退出。

| 字段 | JSON 类型／单位 | 用途、默认值、校验与兼容 |
| --- | --- | --- |
| `timestamp` | RFC 3339 UTC 字符串／毫秒 | 必需事件时间；保留已有字段。 |
| `level` | `trace`、`debug`、`info`、`warn`、`error` | 必需级别；保留现有过滤顺序。 |
| `source` | 字符串／来源 ID | 必需，1–48 个 ASCII 字母、数字、`_` 或 `-`。 |
| `core` | 字符串／核心 ID | 必需运行时所有者（`isos`、`eos` 或 `egia`）；MuJoCo 子进程事件使用 `eos`，`source:mujoco`。 |
| `node_path` | 字符串或 null／规范路径 | 可选所属节点；全局事件为 null 或省略。 |
| `event_code` | 字符串／机器码 | 必需稳定 snake_case，1–64 字符；旧读取器仍接受缺省。 |
| `run_id` | 字符串／不透明 ID | 必需所有者会话身份；部署前用管理器会话 ID，部署后用部署运行 ID；旧 `log_append` 规范化时由 IsOS 补齐。 |
| `correlation_id` | 字符串或 null／不透明 ID | 可选请求关联，不嵌入密钥或用户文本。 |
| `message` | 字符串／UTF-8 字节 | 必需人类说明，最多 8192 字节，写前脱敏。 |
| `fields` | 对象／有界元数据 | 可选，只含标量，最多 16 键和 4 KiB 编码大小；拒绝密钥、含凭证路径及无界负载。 |
| `pid` | 整数或 null／进程 ID | 可选已有进程元数据。 |

```json
{"timestamp":"2026-11-01T10:00:00.000Z","level":"info","source":"eos","core":"eos","node_path":null,"event_code":"runtime_ready","run_id":"r-7c2a","correlation_id":null,"message":"EOS runtime ready","fields":{"state":"ready"},"pid":4242}
```

用 `isos logs all --level info` 或 `GET /api/isos/v1/logs?source=all&limit=100` 读取，随后用 `next_cursor` 轮询。每次读取最多 200 行或 64 KiB；`rotated:true` 表示游标对应文件已轮转，UI 要显示空档。当前默认值为每文件 256 KiB、两份备份、32 个来源文件、最低 `info`。超过来源文件上限的新来源仍写统一流。就绪日志不代表实时健康；`isos status --json` 才是当前状态依据。

**目标接收算法：**校验来源、级别、长度；附上经过认证的所有者和运行上下文；删除密钥字段与凭证样式值；限制字段；在锁内向来源和统一流写一条 JSON 行并轮转；慢速 UI 读取不得阻塞。丢弃／拒绝事件增加可观察计数，警告限速以防递归。HTTP 与 CLI 在存储后过滤。已有 `log_append` 仍是仅所有者可访问的兼容入口；未来跨核心传输可新增字段，不要求旧调用方发送。
