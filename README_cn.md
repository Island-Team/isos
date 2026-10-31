# IsOS

IsOS 管理共享 Aether v1 图、后端软件包、本地工作台 API 和直接子进程生命周期。MuJoCo 是首个后端包。EOS 读取图、启动经 IsOS 校验的 MuJoCo 子进程，并通过有类型的套接字适配器连接它。EGIA common 节点目前用于图设计，不启动 EGIA 进程。

## 启动 MuJoCo 部署

从 v2 源码目录运行 `./install.sh` 安装 CLI，默认不会安装或启动 systemd 用户单元。直接运行 `isos` 会初始化缺失的开发图、启动本地工作台并打开浏览器；后端核心保持停止。再次运行会复用工作台。`isos init --mode dev` 仅准备图，不启动进程；`isos init --mode product` 选择产品模式规则。`isos serve` 在前台运行工作台但不打开浏览器；开发模式可显式使用 `isos serve --ui-live` 启动受管理的 Webpack。

在受版本管理的 [config/isos.toml](config/isos.toml) 中设置可公开的运行默认值；可选字段见 [config/isos.example.toml](config/isos.example.toml)。相对源码路径以 IsOS 源码目录为基准，与命令工作目录无关。环境变量覆盖配置文件值；`ISOS_CONFIG` 可指定其他绝对配置文件路径。私有部署图保存在 `~/.config/isos/`。IsOS 不从命令工作目录读取 `.env`。在 EOS、EGIA 和 IsosUI 各自的仓库构建，再于此文件配置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 和 `paths.ui_dist`；MuJoCo 仍是独立的 IsOS 软件包：

```sh
isos
```

`isos version check` 报告已配置核心路径、产品版本、兼容性、图配置和运行状态；`--json` 为脚本输出同一报告。`ISOSUI_REPO_PATH` 或 `[paths].ui_repo` 只供显式 `isos serve --ui-live` 使用。

`isos` 与 `isos serve` 在 `http://127.0.0.1:16001/en/isos` 提供 IsosUI，在 `http://127.0.0.1:16000/api/isos/v1` 提供 API。IsOS 保留 16000–16019，EOS 保留 16020–16059（控制 16020、运行时目录 16021），EGIA 保留 16060–16099（北向接口 16060）。可显式覆盖监听端口；启动前会拒绝端口冲突。产品模式提供已配置且经校验的静态 UI，并要求兼容的核心程序。Ctrl+C 或 `isos stop` 会停止 UI、API、部署、显式启动的服务及管理器；`isos deployment stop` 只停止图部署。

使用工作台时请保持 `isos serve` 终端运行。保存并运行、重启或图启动被拒绝只控制部署；不需要 systemd 用户服务，工作台仍保持可用。

可选的 systemd 用户单元运行相同的产品模式命令：在 `isos init --mode product` 并配置经过验证的核心与界面路径后，运行 `./install.sh --install-user-service` 安装 CLI 并启用、启动该单元；如果 CLI 已安装，也可以运行 `tools/user-service.sh install`。运行 `tools/user-service.sh remove` 可禁用。CLI 与 shell 补全不依赖此单元。


在第二个终端执行：

`isos open` 会在必要时启动工作台，输出 UI 地址并打开浏览器，不会把桌面工具包的诊断信息转发到终端。

```sh
isos open
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
# 在 Graph 工作台添加已安装的 MuJoCo 节点集合，再校验并应用。
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run` 会在工作台尚未运行时先启动工作台，再通过按需核心适配器准备已保存的图，并按依赖顺序启动已配置的核心。它保持前台运行。按 Ctrl+C 会按依赖逆序停止部署。源码修改应在所属仓库构建，必要时更新已配置路径。可在其他终端执行 `isos status --json`、`isos version check`、`isos services status` 或 `isos stop`。

`isos init` 仅在图不存在时创建可编辑图。Home 展示软件包准备情况和修订；Graph 编辑唯一的 `1.0.0` IsOS/EOS/EGIA 图，节点类型有独立版本。Check graph 校验草稿；Save graph 应用修订并生成核心配置；Run 会重新校验固定版本的软件包、图、场景、可执行文件、资源和 MuJoCo 依赖。仅安装或链接不会改变运行中部署。工作台有单独的 EOS 和 EGIA 页面；EGIA 运行控制留待后续实现。

新图包含 EOS 进程节点和调度器；操作员在 Graph 中添加已安装软件包的节点集合后再应用。已有编写图会保留。对旧 MuJoCo 图，节点集合预览可显式替换旧后端、硬件及依赖它们的控制节点，然后由操作员应用返回的草稿。

默认路径为 `~/.config/isos/deployments/<name>.nodes.json`、`~/.local/share/isos/packages/` 和 `$XDG_RUNTIME_DIR/isos/`。设置 `ISOS_HOME` 可创建隔离测试目录，其中有 `config/`、`data/` 和 `run/`。仅所有者可访问的轮转 JSON 日志位于 `~/.local/share/isos/logs/<deployment>/`；`GET /api/isos/v1/logs` 和界面默认显示统一日志。`config/isos.toml` 中的 `[logging]` 控制默认来源、写入级别、单文件轮转、备份数量和来源文件上限。界面默认显示 `info` 及以上级别。服务 TOML 与 CLI 契约见[服务接口](docs_cn/isos/interfaces/services_cn.md)。

归档包可先执行 `isos pkg verify <manifest> <asset>`，再用 `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` 安装本地候选包。发布后可用 `isos pkg install island/mujoco-sim --version 2.0.0` 安装已批准的软件包。已应用图仍引用软件包时，`isos pkg remove` 会拒绝删除。

参见[后端设计](docs_cn/isos/design/backend-v2_cn.md)、[图接口](docs_cn/isos/interfaces/node-graph-v1_cn.md)、[本地 API](docs_cn/isos/interfaces/local-api_cn.md)以及[英文 README](README.md)。开发检查为 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`；集成输出保存到 `logs/test/<run-id>/`。
