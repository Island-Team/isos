# 集成路线图

[English](../../../docs/isos/maintenance/roadmap.md)

IsOS 0.1 已实现可工作的管理器核心。[平台集成计划](plan_cn.md)区分已完成的适配与发布关卡。以下步骤用于完成 Island 平台集成，属于后续计划，并非本版本已经具备的功能。

1. 将 EGIA 接入明确的 EOS2 契约，保留由 IsOS 管理的无界面启动模式，并用相同的请求、响应、错误示例测试双方。
2. 将 EOS 预启动逻辑移入 IsOS 图中的 service、backend、skill 节点。EOS CLI 保持为可连接的客户端。淘汰兼容包装器前，先将旧操作接口迁移到版本化本地管理 API。
3. 通过 Island Port 发布获批准的 v4 发行清单及对应 GitHub Release 资产，验证 SHA-256、平台信息、可复现构建证据以及安装和回滚关卡。
4. 增加 Python 3.12 外部技能工作进程，允许选择解释器、uv 或 Conda 环境。在隔离进程内使用 pybind11 调用现有原生 C++ 技能 ABI。
5. IsOS 的生命周期、软件包选择、策略与配置图校验继续使用 Rust；性能敏感的 EOS/EGIA 运行时及原生技能继续使用 C++。Rust 与 C++ 优先通过受控进程和版本化 IPC 通信；只有测量证实必须进程内调用时才使用狭窄的 C ABI。不得跨 ABI 传递 C++ 对象、Rust 所有权或 Python 对象。
6. 完成来自托管站点的 WebUI 配对测试，并在 EGIA 的 EOS 2 契约移植后迁移旧 GECA 操作客户端。IsOS 配置图编辑器与软件包详情已在 Island-web 实现。

发布前应在各仓库执行格式、静态检查和测试；在支持的 Linux 平台验证全新安装；检查 EOS 连接/关闭及 EGIA 托管模式；核对中英文任务指南和接口示例。
