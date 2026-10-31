# IsOS 与 MuJoCo v2 入门

[English](../../../docs/isos/usage/get-started.md)

安装 IsOS 后初始化开发环境。`init` 只在配置和起始图不存在时创建它们，然后退出；不会启动管理器或界面。

```sh
isos init --mode dev
isos build eos
isos version select eos <暂存提交或源码指纹>
isos build isosui
isos version select isosui <暂存提交>
isos serve
```

`isos serve` 在前台运行管理器、API 和经验证的静态 IsosUI。在另一终端执行 `isos open`。仅开发前端时，使用 `isos serve --ui-live` 启动 IsosUI 源码目录中已安装依赖的 npm 服务。产品模式要求选择干净构建，无须 npm 或源码目录；详见 [README](../../../README_cn.md)。

构建 MuJoCo 仿真器，用其仓库的 `tools/build-backend-bundle.py` 制作 `2.0.0` 包，并将 `MUJOCO_ROOT` 指向匹配的运行环境。链接包后在 Graph 工作台预览 `sim-hardware` 节点集合。起始图包含 EOS 进程和调度器节点，不包含硬件包。预览加入 MuJoCo 进程、相机、底盘、机械臂、闸门与 Vision 节点。选择场景、检查草稿，再执行 Validate 和 Apply。对应命令为：

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`run`、`start` 和 `restart` 保持前台运行，Ctrl+C 停止部署。在另一终端执行 `isos status --json` 查看管理器和运行中修订；`isos version check` 比较配置、选中与运行中的版本。`isos deployment stop` 只停止图部署；`isos stop` 还停止服务、界面、API 和管理器。重复停止也会成功。

运行前会重新校验精确的图修订、可执行文件收据、软件包、场景和资源。EOS 启动并持有 MuJoCo 子进程。安装软件包、暂存构建或修改草稿都不会改变运行中的部署；需显式选择或应用并重启。参见[软件包命令](packages_cn.md)、[图字段](../interfaces/node-graph-v2_cn.md)和[独立服务契约](../interfaces/services_cn.md)。

若重启时新子进程启动失败，IsOS 会停止本次启动，并按先前的进程配置重新启动先前的运行中修订。若先前的子进程也无法恢复，状态会显示部署已降级及重启错误。
