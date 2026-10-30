# IsOS

IsOS 管理共享 Aether v2 图、后端软件包、本地工作台 API 和直接子进程生命周期。MuJoCo 是首个后端包。EOS 读取图、启动经 IsOS 校验的 MuJoCo 子进程，并通过有类型的套接字适配器连接它。EGIA common 节点目前用于图设计，不启动 EGIA 进程。

## 启动 MuJoCo 部署

从 v2 `master` 源码目录运行 `./install.sh` 安装 CLI。`isos init --mode dev` 创建配置与初始图后退出；再次执行 `isos init` 会保留模式和已编辑的图。`isos init --mode product` 选择产品模式的准入规则。`isos serve` 在前台运行管理器、API 与固定的静态 IsosUI；开发模式可显式使用 `isos serve --ui-live` 启动受管理的 Webpack。

将 [config.example.toml](config.example.toml) 复制到 `~/.config/isos/config.toml`，按需填写绝对源码路径；环境变量覆盖配置文件值。`ISOS_CONFIG` 可指定其他绝对配置路径。IsOS 不从命令工作目录读取 `.env`。开发模式先构建不可变产物，再显式选择：

```sh
isos init --mode dev
isos build eos
isos version select eos <staged-commit>
isos build isosui
isos version select isosui <staged-commit>
isos serve
```

`isos serve` 在 `http://127.0.0.1:16001/en/isos` 提供 IsosUI，在 `http://127.0.0.1:16000/api/isos/v2` 提供 API。`ISOSUI_PORT` 与 `ISOS_HTTP_PORT` 可修改环回端口。产品模式只使用经校验的固定静态 UI，不需要 npm 或源码目录。Ctrl+C 或 `isos stop` 会停止 UI、API、部署、显式启动的服务及管理器；`isos deployment stop` 只停止图部署。

可选的 systemd 用户单元运行相同的产品模式命令：在 `isos init --mode product` 并选择经验证的构建后，运行 `tools/user-service.sh install`；运行 `tools/user-service.sh remove` 可禁用。CLI 与 shell 补全不依赖此单元。

在第二个终端执行：

```sh
isos open
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
# 在 Graph 工作台添加已安装的 MuJoCo 节点集合，再校验并应用。
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run`（以及 `start`、`restart`）会保持前台运行。按 Ctrl+C 会按依赖逆序停止部署。可在其他终端执行 `isos status --json`、`isos version check`、`isos services status` 或 `isos stop`。

`isos init` 仅在图不存在时创建可编辑图。Home 展示软件包准备情况和修订；Graph 编辑唯一的 `2.0.0` IsOS/EOS/EGIA 图，当前节点类型版本为 `v1.0`。Validate 与 Apply 保存修订；Run 会重新校验固定版本的软件包、图、场景、可执行文件、资源和 MuJoCo 依赖。仅安装或链接不会改变运行中部署。工作台有单独的 EOS 和 EGIA 页面；EGIA 运行控制留待后续实现。

新图包含 EOS 进程节点和调度器；操作员在 Graph 中添加已安装软件包的节点集合后再应用。已有编写图会保留。对旧 MuJoCo 图，节点集合预览可显式替换旧后端、硬件及依赖它们的控制节点，然后由操作员应用返回的草稿。

默认路径为 `~/.config/isos/deployments/<name>.nodes.json`、`~/.local/share/isos/packages/` 和 `$XDG_RUNTIME_DIR/isos/`。设置 `ISOS_HOME` 可创建隔离测试目录，其中有 `config/`、`data/` 和 `run/`。仅所有者可访问的轮转 JSON 日志位于 `~/.local/share/isos/logs/<deployment>/`；`GET /api/isos/v2/logs?source=all` 使用有界游标读取统一日志。UI 默认显示 `warn` 及以上级别，写入级别由 `[logging].level` 控制。服务 TOML 与 CLI 契约见[服务接口](docs_cn/isos/interfaces/services_cn.md)。

归档包可先执行 `isos pkg verify <manifest> <asset>`，再用 `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` 安装本地候选包。发布后可用 `isos pkg install island/mujoco-sim --version 2.0.0` 安装已批准的软件包。已应用图仍引用软件包时，`isos pkg remove` 会拒绝删除。

参见[后端设计](docs_cn/isos/design/backend-v2_cn.md)、[图接口](docs_cn/isos/interfaces/node-graph-v2_cn.md)、[本地 API](docs_cn/isos/interfaces/local-api_cn.md)以及[英文 README](README.md)。开发检查为 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`；集成输出保存到 `logs/test/<run-id>/`。
