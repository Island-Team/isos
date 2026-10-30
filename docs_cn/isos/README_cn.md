# IsOS 指南

IsOS 在本地管理 Aether v2 图、后端软件包、工作台 API 和直接子进程，并在图外管理显式启动的模型类服务。首个完整后端目标为 `island/mujoco-sim@2.0.0`。先看 [README](../../README_cn.md)，再看[入门](usage/get-started_cn.md)、[后端设计](design/backend-v2_cn.md)、[图字段](interfaces/node-graph-v2_cn.md)、[服务契约](interfaces/services_cn.md)、[软件包流程](usage/packages_cn.md)、[本地 API](interfaces/local-api_cn.md)和[开发检查](develop/backend-v2_cn.md)。

旧组件图和 systemd 命令保留为兼容入口；可选的用户单元可以运行产品模式的 `isos serve`。v2 主链使用唯一的 `2.0.0` 图，当前节点类型为 `v1.0`。IsOS 校验 MuJoCo 软件包并启动 EOS；EOS 启动 MuJoCo 并连接适配器；EGIA common 节点目前仅供设计。

[English](../../docs/isos/README.md)
