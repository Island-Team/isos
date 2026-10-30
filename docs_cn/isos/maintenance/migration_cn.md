# 从旧启动器迁移

[English](../../../docs/isos/maintenance/migration.md)

旧版 EOS、EGIA 启动器与 IsOS 组件图仍是兼容入口。当前 IsOS 部署使用 [v2 节点图](../interfaces/node-graph-v2_cn.md)、精确构建收据和[独立服务契约](../interfaces/services_cn.md)。当前没有自动导入 TOML 或旧图的功能，也没有 `isos config migrate` 命令。

1. 备份旧版 stack、EOS、EGIA、模型服务及 `<name>.json` 文件。
2. 执行 `isos init --mode dev` 或 `isos init --mode product`。已有的 v2 编辑图会被保留。
3. 构建并选择精确的 IsOS、EOS 和 IsosUI 产物。用 `isos version check` 检查选中和运行中的身份。激活新的 IsOS 可执行文件前须停止管理器。
4. 安装或链接精确版本的软件包，将其节点集合预览到 v2 草稿，校验并应用图。模型类服务放在图外各自的 TOML 文件中。
5. 停止旧启动器。先运行 `isos serve`，再在另一终端运行 `isos run`。检查 `isos status --json` 和日志。`isos deployment stop` 只停止图部署；`isos stop` 关闭管理器及所有所属子进程。

不要让旧版和 v2 管理器同时持有相同端口或进程。IsOS 不会自动移动模型权重或改写旧配置。
