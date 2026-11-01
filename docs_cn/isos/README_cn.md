# IsOS 指南

IsOS 在本地管理已保存图、后端软件包、工作台 API 和直接子进程，并在图外管理显式启动的模型类服务。首个集成目标为 `island/mujoco-sim@0.1.1`。按首次使用顺序阅读：[启动与日志](usage/get-started_cn.md)、[0.x 架构](design/system_cn.md)、[实现里程碑](maintenance/roadmap_cn.md)、[节点目录](layers/README_cn.md)、[图字段与兼容](interfaces/node-graph_cn.md)、[软件包流程](usage/packages_cn.md)、[发行契约](interfaces/releases_cn.md)、[本地 API 与选择器](interfaces/local-api_cn.md)、[配置、版本与日志](interfaces/configuration_cn.md)、[独立服务](interfaces/services_cn.md)、[源码布局](develop/source-layout_cn.md)和[开发检查](develop/backend_cn.md)。

已保存的 `.nodes.json` 图是唯一部署路径；可选用户单元可运行产品模式的 `isos serve`。**当前程序读取 `1.0.0`；拟议写入器使用 `0.1.0`，兼容读取器接受两者。**节点类型独立版本化。IsOS 校验 MuJoCo 软件包并启动 EOS；EOS 启动 MuJoCo 并连接适配器；EGIA common 节点当前仅供设计。各契约均标记目标行为，不能当作已可用命令。

[English](../../docs/isos/README.md)
