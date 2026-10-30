# IsOS

IsOS 管理共享 Aether v2 图、后端软件包、本地工作台 API 和直接子进程生命周期。MuJoCo 是首个后端包。EOS 只读取图，并作为有类型的套接字客户端连接 MuJoCo。EGIA common 节点目前用于图设计，不启动 EGIA 进程。

## 启动 MuJoCo 部署

构建并安装 IsOS 和 Aether Rust 可执行文件。在 MuJoCo 仓库构建仿真器，并用 `tools/build-backend-bundle.py` 制作 v2 包。把 `MUJOCO_ROOT` 指向已安装的 MuJoCo 运行环境，然后执行：

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle
isos init
```

`isos init` 会在当前终端保持工作台和进程管理器运行。请保持此终端开启，在第二个终端编辑图并执行部署命令。按 Ctrl+C 会停止服务和受管理的子进程。

在第二个终端执行：

```sh
isos open
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run`（以及 `start`、`restart`）会保持前台运行。按 Ctrl+C 会按依赖逆序停止部署。运行期间可在第三个终端执行 `isos status --json`、`isos logs simulator` 或 `isos stop`。

`isos init` 仅在图不存在时创建可编辑图，并启动本地工作台服务。Home 展示软件包准备情况和修订；Graph 编辑唯一的 `2.0.0` IsOS/EOS/EGIA 图，当前节点类型版本为 `v1.0`。Validate 与 Apply 保存修订；Run 会重新校验固定版本的软件包、图、场景、可执行文件、资源和 MuJoCo 依赖。仅安装或链接不会改变运行中部署。工作台有单独的 EOS 和 EGIA 页面；EGIA 运行控制留待后续实现。

默认路径为 `~/.config/isos/deployments/<name>.nodes.json`、`~/.local/share/isos/packages/` 和 `$XDG_RUNTIME_DIR/isos/`。设置 `ISOS_HOME` 可创建隔离测试目录，其中有 `config/`、`data/` 和 `run/`。仅所有者可访问的控制套接字位于运行目录；浏览器 API 监听 `127.0.0.1:16000`。本地 IsosUI 可独立固定版本，也可由 `isos open` 使用托管编辑器。

归档包可先执行 `isos pkg verify <manifest> <asset>`，再用 `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` 安装本地候选包。发布后可用 `isos pkg install island/mujoco-sim --version 2.0.0` 安装已批准的软件包。已应用图仍引用软件包时，`isos pkg remove` 会拒绝删除。

参见[后端设计](docs_cn/isos/design/backend-v2_cn.md)、[图接口](docs_cn/isos/interfaces/node-graph-v2_cn.md)、[本地 API](docs_cn/isos/interfaces/local-api_cn.md)以及[英文 README](README.md)。开发检查为 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`；集成输出保存到 `logs/test/<run-id>/`。
