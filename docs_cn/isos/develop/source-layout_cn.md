# IsOS 源码布局

[English](../../../docs/isos/develop/source-layout.md)

`src/main.rs` 初始化配置、解析命令并分发。新增行为按以下职责放置：

| 路径 | 职责 |
| --- | --- |
| `src/cli/mod.rs`、`src/cli/commands.rs` | 对外命令语法与命令处理；调用领域模块和管理器客户端，不持有子进程。 |
| `src/manager/mod.rs`、`src/manager/protocol.rs` | 仅所有者可访问的套接字协议、部署与服务请求分发、管理器客户端；状态变更前校验有界协议字段。 |
| `src/foreground.rs` | 工作台界面、浏览器启动、前台生命周期与信号处理。 |
| `src/config.rs`、`src/core_setup.rs`、`src/readiness.rs` | 运行路径与设置、核心配置文件及启动就绪检查。 |
| `src/process.rs`、`src/backend.rs`、`src/starter.rs`、`src/core_bins.rs` | 图进程监管、已验证后端输入、起始图参数及核心程序路径。 |
| `src/log_store.rs`、`src/manager_log.rs` | 有界结构化日志存储和管理器事件采集。 |
| `src/http.rs` | 环回 HTTP API 和静态界面服务；调用管理器与领域接口，不监管子进程。 |
| `src/packages.rs`、`src/services.rs`、`src/auth.rs`、`src/release.rs`、`src/node_sets.rs`、`src/node_options.rs`、`src/isosui.rs` | 软件包、服务、账号、发布、图目录及界面包领域。 |

`component.rs`、`graph.rs` 和 `versions.rs` 仍支持旧组件命令。最后一个调用者替换且等效测试通过后再删除。公开图、软件包及线协议保留版本号；内部模块与测试文件按职责命名。

统一日志 API 为 `GET /api/isos/v2/logs`。`config/isos.toml` 设置默认来源与保留上限。来源文件达到 `logging.max_sources` 后，新来源的事件仍写入有界统一流。
