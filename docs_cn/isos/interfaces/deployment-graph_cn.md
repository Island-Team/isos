# 旧版组件图

[English](../../../docs/isos/interfaces/deployment-graph.md)

本文描述兼容命令 `isos eos`、`isos egia` 和 `isos daemon` 使用的旧版 `<name>.json` 组件图，并非已应用的 v2 部署图。新工作使用位于 `<IsOS 配置目录>/deployments/<name>.nodes.json` 的[共享 v2 节点图](node-graph-v2_cn.md)及[本地 v2 API](local-api_cn.md)。

`isos config show` 显示正在使用的 v2 图；`isos config validate <文件>` 和 `isos config apply <文件>` 校验与应用该图。`isos version select eos <选择器>` 通过修订检查更新图中的三个 EOS 可执行文件路径。模型类独立服务使用[带版本的 TOML 文件](services_cn.md)，不使用图中的服务节点。当前没有 `isos config migrate` 命令；迁移旧图需明确编写 v2 草稿、校验并应用，且保留旧 JSON 的备份。
