# 启动 IsOS 部署

IsOS 使用 JSON 部署图选择软件包并启动 EOS、EGIA 和它们的依赖。Linux 首版需要 Rust 1.98 或更新版本，以及 systemd 用户会话：

    cargo install --path .
    isos doctor
    isos config init

初始文件位于 ~/.config/isos/deployments/default.json。填写 EOS、north HTTP 附属服务与 EGIA 的 launch.command、EOS 的 north_http_url，然后启用节点。字段见 [部署图契约](../interfaces/deployment-graph_cn.md)。Python 组件默认使用 Python 3.12；可以在 launch.command 中选择 Conda、uv 或明确的解释器路径，IsOS 不固定系统 Python。模型服务可使用 /home/user/.conda/envs/model-py312/bin/python，或 uv run --python 3.12 --project /path/to/service python 作为命令开头；启动前先准备好环境。

    isos config validate ~/.config/isos/deployments/default.json
    isos config apply ~/.config/isos/deployments/default.json
    isos start
    isos status
    isos eos enter

config apply 保存格式化 JSON 并生成 TOML，不会重启正在运行的进程。需要替换当前部署时明确执行 isos restart。isos eos start 只启动 EOS 和它的依赖；isos eos enter 只连接已运行的 EOS。在 EOS CLI 中，shutdown 关闭本部署拥有的 EOS 相关进程；leave、quit、exit 只退出客户端。

接下来可阅读 [软件包操作](packages_cn.md)；旧部署参见 [迁移指南](../maintenance/migration_cn.md)。

[English](../../../docs/isos/usage/get-started.md)
