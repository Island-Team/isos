# IsOS 指南

IsOS 在本地管理已保存图、后端软件包、工作台 API 和直接子进程，并在图外管理显式启动的模型类服务。首个集成目标为 `island/mujoco-sim@0.1.1`。按首次使用顺序阅读：[启动与日志](usage/get-started_cn.md)、[0.x 架构](design/system_cn.md)、[实现里程碑](maintenance/roadmap_cn.md)、[节点目录](layers/README_cn.md)、[图字段与兼容](interfaces/node-graph_cn.md)、[软件包流程](usage/packages_cn.md)、[发行契约](interfaces/releases_cn.md)、[已验证软件包准入](interfaces/package-verification_cn.md)、[本地 API 与选择器](interfaces/local-api_cn.md)、[配置与版本](interfaces/configuration_cn.md)、[统一部署事件](interfaces/events_cn.md)、[当前运行日志游标](interfaces/log-cursors_cn.md)、[独立服务](interfaces/services_cn.md)、[源码布局](develop/source-layout_cn.md)和[开发检查](develop/backend_cn.md)。

已保存的 `.nodes.json` 图是唯一部署路径；可选用户单元可运行产品模式的 `isos serve`。**当前程序写入 `0.1.0`、读取安全的 `1.0.0` 与 `0.1.0`，并提供按修订号检查的显式迁移。**节点类型独立版本化。IsOS 校验 MuJoCo 软件包并启动 EOS；EOS 启动 MuJoCo 并连接适配器；EGIA common 节点当前仅供设计。P01 [已验证软件包契约](interfaces/package-verification_cn.md)说明下一步完整性边界。后续目标行为在各契约中标明。

R03 的[托管控制授权契约](interfaces/control-grants_cn.md)和[启动设计](design/control-grants_cn.md)记录已合并所有者 PR 且通过干净 home 重放的 IsOS 签发与 EOS 交接。较广的 I4 生命周期门槛以及 R05/R06 MuJoCo 实体验证仍未关闭。

[English](../../docs/isos/README.md)
