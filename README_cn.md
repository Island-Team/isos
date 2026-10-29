# IsOS

IsOS 是 Island EOS 和 EGIA 的本地部署管理器，负责配置图、软件包、版本选择和进程生命周期。EOS 与 EGIA 的配置接口可独立调用，无须启动其运行时。

## 安装与入门

需要 Linux、systemd 用户会话、Rust 1.98 或更新版本，以及独立构建的 Island-web IsOS 页面包。本地开发时，将 `.example.env` 复制为 `.env`，配置 EOS 和 EGIA 源码路径。

    ./build.sh
    ./install.sh
    isos init
    isos open
    isos build eos
    isos build egia
    isos version pin eos <精确提交或版本>
    isos version pin egia <精确提交或版本>
    isos version check

`init` 创建默认禁用的 v2 配置图，启动 `127.0.0.1:16000` 上的 IsOS 本地服务。`open` 通过临时令牌打开编辑器。页面只编辑配置，使用明确的“校验”和“应用”按钮。选择版本后启用 `eos`、`eos-uds` 和 `egia` 节点。`eos.uds` 是 EOS 进程上的逻辑端点，不是第二个 EOS 服务。IsOS 可在 EOS、EGIA 停止时渲染并编辑配置。EOS 自带演示绑定；其中的动作需要仿真服务才能就绪。

    isos config validate ~/.config/isos/deployments/default.json
    isos run
    isos status
    isos stop

`run` 校验已选构建的摘要和 EOS/EGIA 协议版本，按 EOS、UDS、EGIA 顺序启动。`stop` 以相反顺序关闭。运行中的部署保留启动时的配置图快照，更新后需明确重启。`isos config migrate` 会备份 v1 图，再转换为 v2；无法映射的字段会被拒绝。`start` 是 `run` 的兼容别名，`packages` 是 `pkg` 的兼容别名。

`isos pkg upload <manifest> <asset>` 校验 v4 清单和文件，上传到已有 Git 标签对应的 GitHub Release 草稿，然后提交 Island Port 审核。提交审核与正式发布是不同状态。

详细说明见 [文档目录](docs_cn/README_cn.md)。
