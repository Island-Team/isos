# 集成路线图

[English](../../../docs/isos/maintenance/roadmap.md)

[集成计划](plan_cn.md)记录已实现的行为。剩余里程碑如下：

1. 在支持的 Linux 目标上验证并发布精确的 IsOS、IsosUI、EOS 和 MuJoCo 产物，包括无需 npm 或源码目录的产品模式全新环境测试。
2. 检查真实 MuJoCo 和已配置的 Island-models 服务，包括故障、停止及后代进程清理。
3. 发布前对跨仓库 API、图、节点类型和服务契约进行版本化及测试。
4. 在契约可用后集成 EGIA 运行控制。当前 EGIA common 节点仅用于设计。
5. 设计未来的服务图表示，同时保持当前独立 TOML 服务管理器不依赖 EOS 或 EGIA 节点。
6. 在调用方迁移后移除旧版组件和 systemd 兼容入口。可选的用户 systemd 单元运行相同的产品模式 `isos serve` 命令。
