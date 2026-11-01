# 首次使用：IsOS 与 MuJoCo

[English](../../../docs/isos/usage/get-started.md)

## 按实际运行顺序阅读

**当前可用：**新图写入 `0.1.0` 和已注册的 `@v0.1` 类型。已知安全的 `1.0.0` 图仍可读取；应用后编辑须显式迁移。设置选择器、统一事件字段和 EGIA 集成属于[后续里程碑](../maintenance/roadmap_cn.md)。操作时先看状态，再查同一阶段的日志；启动事件记录状态变化，状态才代表当前事实。

1. 安装 IsOS 后运行 `isos`。它会创建缺失的开发配置和图，在 16000 启动 API、16001 启动 IsosUI，并打开浏览器；EOS、EGIA 与 MuJoCo 仍停止。再次运行会复用工作台。`isos init` 只初始化文件并退出。

```sh
isos
```

2. 在 EOS、EGIA 与 IsosUI 各自的仓库构建，在 IsOS 仓库的 `config/isos.toml` 中设置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 与 `paths.ui_dist`。执行 `isos version check`（脚本可加 `--json`）。先修复报告的程序缺失或 setup 不匹配，再启动硬件。Rust EOS gateway 完成前，EGIA 可能仍显示阻塞。`isos serve` 不打开浏览器；`isos open` 会按需启动。`isos serve --ui-live` 仅用于前端开发。

新的 `0.1.0` 草稿在 Graph 中显示保存修订号。先 Validate，再用该修订号 Apply。目标图文件不存在时，首次 API PUT 须显式携带空修订号（`"revision":""`）；`isos init` 已保存起始图后则须使用其实际 SHA-256。CLI Apply 对已保存起始图或后续修改使用 `--revision <saved-sha256>`；只有目标文件不存在时才使用 `--revision ""`。归属方 reader 不可用会阻止 Apply，草稿字节保持不变。

3. 构建 MuJoCo 仿真器，用其仓库的 `tools/build-backend-bundle.py` 制作 `0.1.1` 包，并将 `MUJOCO_ROOT` 指向匹配环境。链接后在 Graph 预览 `sim-hardware`。起始图只有 EOS 进程和调度器，没有硬件包；预览加入 MuJoCo 进程、相机、底盘、机械臂、闸门与 Vision 节点。选可用场景，检查草稿，在 Graph 中 Validate 后 Apply。仅安装或链接不会修改图。以下 CLI 只用于链接及列出包；CLI 预览见[软件包操作](packages_cn.md)：

```sh
isos pkg link island/mujoco-sim 0.1.1 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
```

4. Apply 后执行 `isos run`，它保持前台运行。在另一终端先用 `isos status --json` 查看保存／活动修订、进程就绪和最近错误，再用 `isos logs all --level info` 查看启动顺序。当前 IsOS 启动行表示启动尝试，状态才确认就绪。MuJoCo 或可加载适配器的软件包加载故障先读 `isos logs packages --level warn`：`package_id`／`package_version` 标识固定发行版，`run_id` 和 `correlation_id` 关联管理器预检失败；再查看命令错误确定修复方法。EOS 与 MuJoCo 来源解释子进程启动。特定故障用 `isos logs eos --level warn` 或 IsosUI Logs 面板调查。日志位于私有数据根目录的 `logs/<deployment>/`；路径、过滤、轮转和游标见[配置与日志](../interfaces/configuration_cn.md)。目标统一事件会增加单条就绪事件和节点字段；当前大多数事件可能没有。

5. Ctrl+C 停止附着的部署。`isos deployment stop` 仅停止图；`isos stop` 也停止独立服务、UI、API 和管理器。重复停止会成功。停后用 `isos status --json` 确认已停止；若停止异常，再查看警告与错误日志。干净停止无需大量周期性消息。

运行前会重新校验精确的图修订、已配置程序、软件包、场景和资源。EOS 启动并持有 MuJoCo 子进程。安装包、修改程序路径或编辑草稿都不会改变运行中的部署；需显式 Apply 和 Restart。参见[软件包命令](packages_cn.md)、[图字段](../interfaces/node-graph_cn.md)及[独立服务契约](../interfaces/services_cn.md)。

对于已应用的 `0.1.0` 图，Run 会在启动前将按修订保存的生成文件与新的归属方 render 比较。如果配置的 EOS 或 EGIA 程序发生变化，`isos init` 会报告过期的启动输入并保留不可变快照。恢复固定的程序，或创建新的图修订并 Apply，然后再运行。

若重启时新子进程启动失败，IsOS 会停止本次启动，并按先前的进程配置重新启动先前的运行中修订。若先前的子进程也无法恢复，状态会显示部署已降级及重启错误。

### 迁移已保存的旧图

编辑已应用的 `1.0.0` 图之前，先从 Graph 或 `GET /api/isos/v1/node-graph` 读取保存修订号。预览会报告精确类型替换、完整的 `0.1.0` 建议图及阻塞原因，且不改变保存字节或运行中的子进程：

```sh
isos config migrate --preview --revision <saved-sha256>
```

检查建议结果并解决归属方 reader 或软件包问题，再用**预览给出的摘要**和两个修订号提交：

```sh
isos config migrate --revision <saved-sha256> --proposed-revision <preview-sha256>
```

来源或建议摘要过期时须重新预览。`isos status --json` 随后显示新的保存修订号；运行中修订号在显式 Restart 前保持不变。未匹配的旧类型保持只读，须按[迁移指南](../maintenance/migration_cn.md#已保存图-100--010-迁移)显式修复。
