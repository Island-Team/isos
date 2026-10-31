# IsOS 源码布局

[English](../../../docs/isos/develop/source-layout.md)

`src/main.rs` 声明所属模块并调用 `app/entry.rs`。新增行为放在对应职责目录：

| 目录 | 职责 |
| --- | --- |
| `src/app/` | CLI、环回 HTTP API、前台工作台、运行配置、认证和已验证的界面包；调用领域与运行接口，但不监管图子进程。 |
| `src/domain/` | 保存图的起始模板、节点集合与选项、软件包、发布、后端描述及独立服务；安装软件包不会启动部署。 |
| `src/runtime/` | 直接子进程管理器和套接字协议、图进程投影与监管、核心配置、程序预检、就绪状态、锁和有界结构化日志。 |

保存的 `<name>.nodes.json` 是唯一部署图。`src/runtime/lock.rs` 对图、生成配置与软件包变更进行串行化。旧组件图、构建选择检查及 systemd 子进程运行时已在调用者迁移后删除。可选的 `isos.service` 与前台操作流程一样运行 `isos serve`。

统一日志 API 为 `GET /api/isos/v1/logs`。`config/isos.toml` 设置默认来源与保留上限。来源文件达到 `logging.max_sources` 后，新来源的事件仍写入有界统一流。
