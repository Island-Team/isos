# 首次使用：IsOS 与 MuJoCo

[English](../../../docs/isos/usage/get-started.md)

## 按实际运行顺序阅读

**当前可用：**以下命令使用现有 `1.0.0` 图读取器和 `@v1.*` 类型。拟议的 `0.1.0` 图写入器、设置选择器、统一事件字段和 EGIA 集成列在[实现 TODO](../design/system_cn.md#按依赖顺序的实现-todo)。操作时先看状态，再查同一阶段的日志；启动事件证明发生过一次状态变化，而状态才代表当前事实。

1. 安装 IsOS 后运行 `isos`。它会创建缺失的开发配置和图，在 16000 启动 API、16001 启动 IsosUI，并打开浏览器；EOS、EGIA 与 MuJoCo 仍停止。再次运行会复用工作台。`isos init` 只初始化文件并退出。

```sh
isos
```

2. 在 EOS、EGIA 与 IsosUI 各自的仓库构建，在 IsOS 仓库的 `config/isos.toml` 中设置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 与 `paths.ui_dist`。执行 `isos version check`（脚本可加 `--json`）。先修复报告的程序缺失或 setup 不匹配，再启动硬件。Rust EOS gateway 完成前，EGIA 可能仍显示阻塞。`isos serve` 不打开浏览器；`isos open` 会按需启动。`isos serve --ui-live` 仅用于前端开发。

3. 构建 MuJoCo 仿真器，用其仓库的 `tools/build-backend-bundle.py` 制作 `0.1.1` 包，并将 `MUJOCO_ROOT` 指向匹配环境。链接后在 Graph 预览 `sim-hardware`。起始图只有 EOS 进程和调度器，没有硬件包；预览加入 MuJoCo 进程、相机、底盘、机械臂、闸门与 Vision 节点。选可用场景，检查草稿，在 Graph 中 Validate 后 Apply。仅安装或链接不会修改图。以下 CLI 只用于链接及列出包；CLI 预览见[软件包操作](packages_cn.md)：

```sh
isos pkg link island/mujoco-sim 0.1.1 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
```

4. Apply 后执行 `isos run`，它保持前台运行。在另一终端先用 `isos status --json` 查看保存／活动修订、进程就绪和最近错误，再用 `isos logs all --level info` 查看启动顺序。当前 IsOS 启动行表示启动尝试，状态才确认就绪；包／图诊断解释验证，EOS 与 MuJoCo 来源解释子进程启动。特定故障用 `isos logs eos --level warn` 或 IsosUI Logs 面板调查。日志位于私有数据根目录的 `logs/<deployment>/`；路径、过滤、轮转和游标见[配置与日志](../interfaces/configuration_cn.md)。目标统一事件会增加单条就绪事件、运行／关联 ID 及节点字段；当前输出可能没有。

5. Ctrl+C 停止附着的部署。`isos deployment stop` 仅停止图；`isos stop` 也停止独立服务、UI、API 和管理器。重复停止会成功。停后用 `isos status --json` 确认已停止；若停止异常，再查看警告与错误日志。干净停止无需大量周期性消息。

运行前会重新校验精确的图修订、已配置程序、软件包、场景和资源。EOS 启动并持有 MuJoCo 子进程。安装包、修改程序路径或编辑草稿都不会改变运行中的部署；需显式 Apply 和 Restart。参见[软件包命令](packages_cn.md)、[图字段](../interfaces/node-graph_cn.md)及[独立服务契约](../interfaces/services_cn.md)。

若重启时新子进程启动失败，IsOS 会停止本次启动，并按先前的进程配置重新启动先前的运行中修订。若先前的子进程也无法恢复，状态会显示部署已降级及重启错误。
