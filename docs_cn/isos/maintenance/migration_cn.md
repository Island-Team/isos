# 从旧组件运行时迁移

[English](../../../docs/isos/maintenance/migration.md)

IsOS 只运行位于 `deployments/<name>.nodes.json` 的[保存节点图](../interfaces/node-graph_cn.md)。旧 `<name>.json` 组件图、构建收据和版本选择不会导入或执行。IsOS 保留这些文件，并提示所需的当前图路径。没有 `isos config migrate` 命令。

1. 备份旧图、stack 和核心配置。若 `isos-server.service` 仍在运行，执行 `systemctl --user stop isos-server.service`。
2. 运行 `isos init` 前，把旧 `<name>.json` 或版本为 `2.0.0` 的 `.nodes.json` 图手动移到备份路径；也可直接编写新的 `1.0.0` 图。只有目标图不存在时，`isos init` 才创建 v1 起始图。检查每个节点、软件包引用与连接；不会自动转换旧图。
3. 在 EOS、EGIA 和 IsosUI 各自的仓库构建。配置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 和 `paths.ui_dist`，再运行 `isos version check`。
4. 安装或链接精确版本的软件包。将节点集合预览到当前图，校验并应用保存的修订。旧 MuJoCo 和程序命令节点格式会被拒绝并给出字段提示。以后重建核心时，`isos init` 可刷新已应用 v1 图的已验证启动输入，而不改变图内容。
5. 运行 `isos serve`，在另一终端运行 `isos run`。检查 `isos status --json` 与统一日志。`isos deployment stop` 停止图；`isos stop` 关闭管理器及其子进程。

可选的当前 `isos.service` 运行 `isos serve`。IsOS 不移动模型权重，也不改写旧配置。
