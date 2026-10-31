# IsOS 指南

IsOS 在本地管理 Aether v1 图、后端软件包、工作台 API 和直接子进程，并在图外管理显式启动的模型类服务。首个完整后端目标为 `island/mujoco-sim@2.0.0`。先看 [README](../../README_cn.md)，再看[入门](usage/get-started_cn.md)、[后端设计](design/backend-v2_cn.md)、[图字段](interfaces/node-graph-v1_cn.md)、[服务契约](interfaces/services_cn.md)、[软件包流程](usage/packages_cn.md)、[本地 API](interfaces/local-api_cn.md)、[源码布局](develop/source-layout_cn.md)和[开发检查](develop/backend-v2_cn.md)。

已保存的 `.nodes.json` 图是唯一部署路径；可选用户单元可运行产品模式的 `isos serve`。图契约为 `1.0.0`，节点类型独立版本化。IsOS 校验 MuJoCo 软件包并启动 EOS；EOS 启动 MuJoCo 并连接适配器；EGIA common 节点目前仅供设计。

[English](../../docs/isos/README.md)
