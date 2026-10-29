# IsOS

IsOS 是 Island EOS 和 EGIA 的本地管理器。它统一管理软件包库、部署配置和进程生命周期； EOS 与 EGIA 仍可独立运行。

目前已实现命令行、JSON 配置图校验、发行清单校验、软件包存储、systemd 用户服务管理、仅允许本机用户访问的 Unix socket API，以及需要配对令牌的本地 WebUI 桥接服务。浏览器页面位于 Island-web；旧 TOML 自动导入属于后续工作。

## 安装与入门

需要 Linux、systemd 用户会话和 Rust 1.98 或更新版本：

    cargo install --path .
    isos doctor
    isos config init

打开 ~/.config/isos/deployments/default.json，填写 EOS/EGIA 的 launch.command、EOS 配置路径和 HTTP 地址，再把需要运行的节点设为 enabled = true。随后执行：

    isos config validate ~/.config/isos/deployments/default.json
    isos config apply ~/.config/isos/deployments/default.json
    isos start
    isos status
    isos eos enter

config apply 会格式化 JSON 并生成 stack.toml，不会自动重启。 需要切换运行中的配置时，明确执行 isos restart。 isos eos enter 只连接已运行的 EOS。EOS CLI 中的 leave、quit、 exit 只退出客户端；shutdown 关闭本部署拥有的 EOS、依赖它的 EGIA 及不再被其他节点使用的服务，并关闭其他由 isos eos enter 打开的会话。

isos skills、isos services 和 isos backends 分别管理技能、模型服务和硬件或仿真后端；它们支持 list、info、status、install、remove。前三个命令查看部署图节点或运行状态，后两个命令按发行包类型管理经过验证的安装。EOS 和 EGIA 属于组件。ISC 是 EOS 内部的技能通信机制，ss 用于查看技能执行。

详细说明见 [文档目录](docs_cn/README_cn.md)。
