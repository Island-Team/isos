# IsOS 开发约定

IsOS 采用 [Island 编码约定 v1.2.0](https://github.com/Island-Team/Island-eos/blob/main/docs_cn/island/conventions.v1.3.0_cn.md)，并根据 Rust 管理器做适配。修改本页时，请同步更新 [英文版](../../docs/isos/conventions.md)。

在首次使用之前介绍 IsOS、EOS、EGIA、软件包及专门术语。操作指南放在 usage，接口契约放在 interfaces，发行和迁移工作放在 maintenance。接口文档说明拥有方、调用方、传输、输入、输出、生命周期和可观察错误，并给出有效的请求、响应示例。英文与中文目录应保持对应。

Markdown、JSON 和配置使用 2 空格缩进；Rust 以 cargo fmt 和官方风格为准。行内注释只解释无法从代码看出的约束。Python 工具默认 Python 3.12，除非所属项目固定了其他受支持版本；IsOS 启动命令可选择 Conda、uv 或明确的解释器，不依赖系统 Python。

提交前运行 cargo fmt --check、cargo clippy --all-targets -- -D warnings 和 cargo test。图排序、软件包校验、进程归属和 API 边界变更应有相应测试。运行状态放在 XDG 路径，不写入源码仓库。使用小而清晰的 Conventional Commits；公开 PR 经审查后可 squash merge。

公开的 IsosUI 仓库负责编辑器源码和静态构建。IsOS 负责 CLI、本地 API 和一个已验证界面文件包的配置路径。不要把编辑器源码复制到本仓库，也不要把 UI 文件放进标准 IsOS 发行包。界面文件包路径独立于 EOS/EGIA 配置图选择。
