# IsOS 节点目录

[English](../../../docs/isos/layers/README.md) · [图契约](../interfaces/node-graph_cn.md) · [首次使用](../usage/get-started_cn.md)

本目录按部署图的 `core → layer → group → node` 层级组织 IsOS 自有节点。序列化字段与兼容规则以[图契约](../interfaces/node-graph_cn.md)为准，生成配置与日志以[配置契约](../interfaces/configuration_cn.md)为准。叶页说明操作员看到什么、启动什么以及故障和停止行为。新的 `@v0.1` 是目标设计；已有 `@v1.*` ID 保留既有行为。

| 核心 | 层 | 组 | 节点类型 | 状态 |
| --- | --- | --- | --- | --- |
| `isos` | `process` | `component` | [受管组件](process/groups/component/nodes/managed-component_cn.md) | 当前 `@v1.1`；目标 `@v0.1` |
| `isos` | `ui` | `panel` | [Vision 面板](ui/groups/panel/nodes/vision_cn.md) | 当前 `@v1.0`；目标补充类型说明 |

IsOS 保存 EOS 与 EGIA 子树，但不拥有其节点行为。EOS 的调度器、控制、硬件与内部 MuJoCo 适配器见 [Island-eos](https://github.com/Island-Team/Island-eos)（`docs/eos/layers/`）；EGIA 阶段见 [Island-egia](https://github.com/Island-Team/Island-egia)（`docs/egia/layers/`）。软件包可在声明的所有者下贡献已注册类型，但不能获得任意进程或执行器权限。模型类独立服务遵循[服务契约](../interfaces/services_cn.md)，本次发布不作为图节点；0.x 目标没有 workflow 节点类型。
