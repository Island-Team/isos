# IsOS

IsOS 管理共享 Aether v2 图、后端软件包、本地工作台 API 和直接子进程生命周期。MuJoCo 是首个后端包。EOS 读取图、启动经 IsOS 校验的 MuJoCo 子进程，并通过有类型的套接字适配器连接它。EGIA common 节点目前用于图设计，不启动 EGIA 进程。

## 启动 MuJoCo 部署

从 v2 `master` 源码目录运行 `./install.sh` 以替换已安装的 IsOS 命令。安装程序会停止旧部署及旧版 `isos-server.service`，删除已废弃的 UI 令牌，并强制安装当前源码构建的可执行文件。随后用 `isos --version` 确认版本为 `2.0.0`。`isos stop` 也能停止残留的旧版 IsOS 服务。v2 的 `isos init` 服务在当前终端运行，按 Ctrl+C 退出。

构建并安装 IsOS 及 Aether Rust 可执行文件（`aether-profile`、`aether-runtime`、`aether-http-bridge`、`aether-config-server`）。先在相邻的 IsosUI 仓库执行一次 `npm ci`；随后 `isos init` 会启动其 `npm start` 服务。将 Aether 可执行文件放在 `isos` 旁边或 `PATH` 中，或者设置绝对路径 `ISOS_AETHER_BIN_DIR`。将 [config.example.toml](config.example.toml) 复制到 `~/.config/isos/config.toml`，按需填写绝对路径；环境变量覆盖配置文件值。`ISOS_CONFIG` 可指定其他绝对配置路径。IsOS 不再从命令工作目录读取 `.env`。在 MuJoCo 仓库构建仿真器，并用 `tools/build-backend-bundle.py` 制作 v2 包，然后执行：

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos init
```

`isos init` 在 `http://127.0.0.1:16001/en/isos` 启动 IsosUI，IsOS 仅在 `http://127.0.0.1:16000/api/isos/v2` 提供 API，无需 URL 令牌。`ISOSUI_PORT` 和 `ISOS_HTTP_PORT` 可分别调整端口；两者都只绑定环回地址。请保持此终端开启，在第二个终端编辑图并执行部署命令。按 Ctrl+C 会停止 npm 前端、API、进程管理器和受管理的子进程。

在第二个终端执行：

```sh
isos open
isos pkg node-sets
# 在 Graph 工作台添加已安装的 MuJoCo 节点集合，再校验并应用。
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run`（以及 `start`、`restart`）会保持前台运行。按 Ctrl+C 会按依赖逆序停止部署。运行期间可在第三个终端执行 `isos status --json`、`isos logs eos` 或 `isos stop`。

`isos init` 仅在图不存在时创建可编辑图，并启动本地工作台服务。Home 展示软件包准备情况和修订；Graph 编辑唯一的 `2.0.0` IsOS/EOS/EGIA 图，当前节点类型版本为 `v1.0`。Validate 与 Apply 保存修订；Run 会重新校验固定版本的软件包、图、场景、可执行文件、资源和 MuJoCo 依赖。仅安装或链接不会改变运行中部署。工作台有单独的 EOS 和 EGIA 页面；EGIA 运行控制留待后续实现。

新图包含 EOS 进程节点和调度器；操作员在 Graph 中添加已安装软件包的节点集合后再应用。已有编写图会保留。对旧 MuJoCo 图，节点集合预览可显式替换旧后端、硬件及依赖它们的控制节点，然后由操作员应用返回的草稿。

默认路径为 `~/.config/isos/deployments/<name>.nodes.json`、`~/.local/share/isos/packages/` 和 `$XDG_RUNTIME_DIR/isos/`。设置 `ISOS_HOME` 可创建隔离测试目录，其中有 `config/`、`data/` 和 `run/`。仅所有者可访问的控制套接字位于运行目录；浏览器 API 监听 `127.0.0.1:16000`，npm 在 `127.0.0.1:16001` 提供 IsosUI。EOS HTTP 桥与只读图目录默认分别监听 `127.0.0.1:16020` 和 `127.0.0.1:16021`。

归档包可先执行 `isos pkg verify <manifest> <asset>`，再用 `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` 安装本地候选包。发布后可用 `isos pkg install island/mujoco-sim --version 2.0.0` 安装已批准的软件包。已应用图仍引用软件包时，`isos pkg remove` 会拒绝删除。

参见[后端设计](docs_cn/isos/design/backend-v2_cn.md)、[图接口](docs_cn/isos/interfaces/node-graph-v2_cn.md)、[本地 API](docs_cn/isos/interfaces/local-api_cn.md)以及[英文 README](README.md)。开发检查为 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`；集成输出保存到 `logs/test/<run-id>/`。
