# IsOS 指南

IsOS 在本地管理 Aether v2 图、后端软件包、工作台 API 和直接子进程。首个完整目标为 `island/mujoco-sim@2.0.0`。先看[入门](../../README_cn.md)，再看[后端设计](design/backend-v2_cn.md)、[图字段](interfaces/node-graph-v2_cn.md)、[软件包流程](usage/packages_cn.md)、[本地 API](interfaces/local-api_cn.md)和[开发检查](develop/backend-v2_cn.md)。

旧组件图和 systemd 命令作为旁支保留，等待剩余调用方重构。v2 主链使用唯一的 `2.0.0` 图，当前节点类型为 `v1.0`。IsOS 启动 MuJoCo；EOS 只读取图并连接它；EGIA common 节点目前仅供设计。

[English](../../docs/isos/README.md)
