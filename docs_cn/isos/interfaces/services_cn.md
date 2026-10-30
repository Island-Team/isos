# 独立服务契约 v2

IsOS 管理模型等服务进程，但不把它们表示为部署图节点。仅所有者可访问的 IsOS 管理器读取 `<IsOS 配置目录>/services/<id>.toml`，每个服务一个文件。调用方是本地 CLI 或管理器操作。只有明确请求时才启动服务；`isos serve` 和 `isos run` 不自动启动服务。停止整个管理器会停止其全部服务。

## 定义

```toml
version = "2.0.0"
id = "gateway"
command = ["/absolute/Island-models/launch.sh", "--managed", "--only", "gateway"]
cwd = "/absolute/Island-models"
depends_on = []
restart = "on_failure"
max_restarts = 2
start_timeout_ms = 30000
stop_timeout_ms = 5000

[artifact]
path = "/absolute/Island-models/launch.sh"
sha256 = "<64 位十六进制 SHA-256>"

[readiness]
kind = "tcp"
target = "127.0.0.1:18003"

[adapter]
kind = "island_models"
repo = "/absolute/Island-models"
alias = "gateway"
```

| 字段 | 类型和单位 | 用途与校验 |
| --- | --- | --- |
| `version` | 字符串 | 必须为 `2.0.0`。 |
| `id` | 字符串 | 1–32 位 ASCII 字母、数字、`_` 或 `-`；与文件名一致。 |
| `artifact.path`, `artifact.sha256` | 绝对文件路径、SHA-256 十六进制 | 精确产物身份；启动前对内容计算哈希。Island-models 使用 `launch.sh`。 |
| `adapter` | 可选带类型表 | `kind = "island_models"` 用绝对路径 `repo` 和 `alias` 检查已安装的目录与预检；结果必须与 `command` 一致。 |
| `command` | 字符串数组 | 可执行文件绝对路径和最多 63 个参数；文件必须可执行。 |
| `cwd` | 绝对目录 | 进程工作目录；必须存在。 |
| `env_from` | 环境变量名映射 | 把管理器环境变量映射为子进程变量；来源必须存在。配置和状态中不保存或显示变量值。 |
| `preflight`, `readiness`, `health` | 探针表 | 可选的准入、启动就绪和运行健康检查。`kind` 为 `tcp`、`unix` 或 `exec`；`target` 为地址、绝对套接字路径或绝对可执行文件路径。`exec` 可使用 `args`。 |
| `depends_on` | 服务 ID 数组 | 仅引用其他服务；循环或缺失依赖会使校验失败。启动时先启动依赖。 |
| `restart`, `max_restarts` | 枚举、次数 | `never`、`on_failure` 或 `always`；最多自动重启 10 次。 |
| `start_timeout_ms`, `stop_timeout_ms` | 毫秒 | 启动就绪期限 1–120000；发送 SIGTERM 后等待 1–60000，再发送 SIGKILL。 |

管理器在启动任何子进程前校验请求涉及的定义和产物。它按依赖顺序启动，启动失败时回滚本次新启动的进程，停止时先停依赖方。已停止的服务只有收到明确的启动或重启请求才会运行。状态包含 ID、状态、PID、重启次数、运行产物的 SHA-256 和可选健康结果。日志写入仅所有者可读的轮转 JSON 行文件，来源为 `service-<id>`，并通过有界游标读取。每个服务拥有独立进程组；停止时向组发送信号，等待配置的宽限时间，必要时强制终止。

公开 CLI 为 `isos services list|validate|start <id>|stop <id>|restart <id>|status [id]|logs <id>`。修改操作通过仅所有者可访问的管理器套接字执行，UID 不同的调用方会被拒绝。`list` 和 `validate` 只读取文件，不启动服务。重复 `start` 返回 `{"started":[]}`，重复 `stop` 返回 `{"stopped":[]}`。`services logs` 返回 `lines`、`next_cursor` 和 `rotated`，与 IsOS 日志游标契约一致。

## Island-models 适配器

适配器读取 `<仓库>/launch.sh --catalog-json`，要求 schema 版本为 1 且包含指定别名，再运行 `--only <alias> --preflight-json`。仅当 `ready:true` 时返回绝对路径形式的 `launch.sh --managed --only <alias>` 命令。IsOS 不修改 Island-models，也不按端口接管进程。服务定义需固定启动器摘要；更新仓库后重新校验。适配器报告预检中的缺失产物和错误，但不暴露环境变量值。

`isos services adapter models <仓库绝对路径> <别名>` 检查并打印托管命令，但不会写入服务文件；操作员需显式创建 TOML 定义并固定摘要。

## 错误

版本不受支持、ID 不合法、环境来源或依赖缺失、循环依赖、摘要不匹配、预检失败、就绪失败、意外退出或健康检查失败，都会成为管理器错误和 IsOS 警告日志。启动失败仅停止本次新启动的进程。对已停止服务重复执行 `stop` 会成功返回空 `stopped` 列表。

[English](../../../docs/isos/interfaces/services.md)
