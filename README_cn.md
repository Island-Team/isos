# IsOS

IsOS 是 Island EOS 和 EGIA 的本地部署管理器，负责配置图、软件包、版本选择和进程生命周期。EOS 与 EGIA 的配置接口可独立调用，无须启动其运行时。

## 安装与入门

需要 Linux、systemd 用户会话和 Rust 1.98 或更新版本。本地开发时，将 `.example.env` 复制为 `.env`，配置 EOS 和 EGIA 源码路径。只有构建可选的本地编辑器时才需要设置 `ISOSUI_REPO_PATH`。

    ./build.sh
    ./install.sh
    isos init
    isos open
    isos build eos
    isos build egia
    isos version pin eos <精确提交或版本>
    isos version pin egia <精确提交或版本>
    isos version check

安装程序将 `isos` 放在 Cargo 的 bin 目录。如果该目录不在 `PATH` 中，但 `~/.local/bin` 在 `PATH` 中，安装程序会在那里创建链接。否则，将安装目录的 bin 路径（通常是 `~/.cargo/bin`）加入 `PATH`，然后重新打开 shell。

IsOS 安装包只包含 CLI 和本地 API 服务。未固定本地构建时，`isos open` 打开 `https://www.os-island.com/en/isos` 上的 IsosUI 编辑器。若要离线使用，请安装 Node.js 和 npm，把 `ISOSUI_REPO_PATH` 指向已提交的 [IsosUI](https://github.com/Island-Team/IsosUI) 源码，然后执行 `isos build isosui` 和 `isos version pin isosui <提交>`。IsOS 将校验过的页面存放在 XDG 数据目录，并从 `127.0.0.1:16000` 提供服务。UI 固定版本独立于 EOS/EGIA 部署图。

`init` 创建默认禁用的 v2 配置图，启动 `127.0.0.1:16000` 上的 IsOS 本地服务。编辑器无需令牌即可连接此端口。页面只编辑配置，使用明确的“校验”和“应用”按钮。选择版本后启用 `eos`、`eos-uds` 和 `egia` 节点。`eos.uds` 是 EOS 进程上的逻辑端点，不是第二个 EOS 服务。IsOS 可在 EOS、EGIA 停止时渲染并编辑配置。EOS 自带演示绑定；其中的动作需要仿真服务才能就绪。

    isos config validate ~/.config/isos/deployments/default.json
    isos run
    isos status
    isos stop

`run` 校验已选构建的摘要和 EOS/EGIA 协议版本，按 EOS、UDS、EGIA 顺序启动。`stop` 以相反顺序关闭。运行中的部署保留启动时的配置图快照，更新后需明确重启。`isos config migrate` 会备份 v1 图，再转换为 v2；无法映射的字段会被拒绝。`start` 是 `run` 的兼容别名，`packages` 是 `pkg` 的兼容别名。

`isos pkg upload <manifest> <asset>` 校验 v4 清单和文件，上传到已有 Git 标签对应的 GitHub Release 草稿，然后提交 Island Port 审核。提交审核与正式发布是不同状态。

详细说明见 [文档目录](docs_cn/README_cn.md)。

## Tab 自动补全

`./install.sh` 会安装 Bash、Zsh 和 Fish 的补全脚本，并在现有 Bash 或 Zsh 启动文件（或登录 shell 的启动文件）中加入加载代码；Fish 会从标准补全目录自动读取脚本。重新打开 shell 后，Tab 可补全 IsOS 命令、子命令和选项。
