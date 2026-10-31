# IsOS 与 MuJoCo v2 入门

[English](../../../docs/isos/usage/get-started.md)

安装 IsOS 后直接运行 `isos`。它会初始化缺失的开发图，在 16000 启动 IsOS API、16001 启动 IsosUI，并打开浏览器；后端核心保持停止。再次运行会复用工作台。`isos init` 只创建缺失的配置和图文件，然后退出。

```sh
isos
```

在 EOS、EGIA 和 IsosUI 各自的仓库构建，然后在 `config/isos.toml` 设置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 和 `paths.ui_dist`。`isos version check` 报告每个核心是否能够运行；脚本可使用 `--json`。MuJoCo 仍是外部 IsOS 软件包。`isos serve` 启动工作台但不打开浏览器；`isos open` 会在必要时启动工作台。仅开发前端时，使用 `isos serve --ui-live` 启动 IsosUI 源码目录中已安装依赖的 npm 服务。`isos build` 和 `isos version select` 现在提供迁移指引，不再构建或选择核心。

构建 MuJoCo 仿真器，用其仓库的 `tools/build-backend-bundle.py` 制作 `2.0.0` 包，并将 `MUJOCO_ROOT` 指向匹配的运行环境。链接包后在 Graph 工作台预览 `sim-hardware` 节点集合。起始图包含 EOS 进程和调度器节点，不包含硬件包。预览加入 MuJoCo 进程、相机、底盘、机械臂、闸门与 Vision 节点。选择场景、检查草稿，再执行 Validate 和 Apply。对应命令为：

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`run`、`start` 和 `restart` 保持前台运行，Ctrl+C 停止部署。在另一终端执行 `isos status --json` 查看管理器和运行中修订；`isos version check` 报告已配置构建、图和运行状态的就绪情况。`isos deployment stop` 只停止图部署；`isos stop` 还停止服务、界面、API 和管理器。重复停止也会成功。

运行前会重新校验精确的图修订、已配置程序、软件包、场景和资源。EOS 启动并持有 MuJoCo 子进程。安装软件包、修改已配置构建路径或编辑草稿都不会改变运行中的部署；需应用修改并显式重启。参见[软件包命令](packages_cn.md)、[图字段](../interfaces/node-graph-v1_cn.md)和[独立服务契约](../interfaces/services_cn.md)。

若重启时新子进程启动失败，IsOS 会停止本次启动，并按先前的进程配置重新启动先前的运行中修订。若先前的子进程也无法恢复，状态会显示部署已降级及重启错误。
