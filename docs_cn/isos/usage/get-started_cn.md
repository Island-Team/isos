# IsOS 与 MuJoCo v2 入门

[English](../../../docs/isos/usage/get-started.md)

安装 IsOS 和 Aether Rust 可执行文件。构建 MuJoCo C++ 仿真器、设置 `MUJOCO_ROOT`，并用 MuJoCo 仓库的 `tools/build-backend-bundle.py` 制作包。链接精确版本后初始化图：

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle
isos init
```

保持 `isos init` 在此终端运行。它负责工作台服务和进程管理器；按 Ctrl+C 会停止服务及其子进程。在第二个终端运行 `isos open` 并编辑图。

Home 展示软件包、场景选择、先决条件、保存修订、运行中修订、进程状态和日志。Graph 编辑唯一的 IsOS/EOS/EGIA 共享图。起始图含后端进程、EOS 运行时、HTTP 桥、目录服务、相机、底盘、机械臂、调度器及仅供设计的 EGIA guard。选择场景并检查可执行文件路径，然后 Validate 和 Apply。对应 CLI 命令为：

```sh
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`run`、`start` 和 `restart` 会保持前台运行。按 Ctrl+C 会请求正常停止部署。部署运行时，可在另一终端执行 `status`、`logs` 或 `stop`。

Run 重新校验精确软件包版本和图，先启动 EOS，再由 EOS 启动 MuJoCo；Stop 通过 EOS 终止仿真器。安装包或修改编辑器草稿不改变运行中进程；须应用并重启以激活新修订。本地链接在每次应用和运行时重新校验。参见[软件包命令](packages_cn.md)与[图字段](../interfaces/node-graph-v2_cn.md)。
