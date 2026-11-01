# 从旧组件运行时迁移

[English](../../../docs/isos/maintenance/migration.md)

IsOS 只运行位于 `deployments/<name>.nodes.json` 的[保存节点图](../interfaces/node-graph_cn.md)。旧 `<name>.json` 组件图、构建收据和版本选择不会导入或执行。IsOS 保留这些文件，并提示所需的当前图路径。`isos config migrate` 只接受安全且已保存的 `1.0.0` `.nodes.json` 图。

1. 备份旧图、stack 和核心配置。若 `isos-server.service` 仍在运行，执行 `systemctl --user stop isos-server.service`。
2. 运行 `isos init` 前，把旧 `<name>.json` 或版本不受支持（如 `2.0.0`）的 `.nodes.json` 图手动移到备份路径；也可直接编写新的 `0.1.0` 图。只有目标图不存在时，`isos init` 才创建 `0.1.0` 起始图。检查每个节点、软件包引用与连接；旧组件图不会自动转换。
3. 在 EOS、EGIA 和 IsosUI 各自的仓库构建。配置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 和 `paths.ui_dist`，再运行 `isos version check`。
4. 安装或链接精确版本的软件包。将节点集合预览到当前图，校验并应用保存的修订。旧 MuJoCo 和程序命令节点格式会被拒绝并给出字段提示。以后重建核心时，`isos init` 可刷新已应用 v1 图的已验证启动输入，而不改变图内容。
5. 运行 `isos serve`，在另一终端运行 `isos run`。检查 `isos status --json` 与统一日志。`isos deployment stop` 停止图；`isos stop` 关闭管理器及其子进程。

可选的当前 `isos.service` 运行 `isos serve`。IsOS 不移动模型权重，也不改写旧配置。

## 已保存图 `1.0.0` → `0.1.0` 迁移

显式操作接受安全且**已保存的 `.nodes.json`** 图，不导入旧 `<name>.json` 组件文件。先读取 `GET /api/isos/v1/node-graph`，记录原始保存字节的修订号。运行 `isos config migrate --preview --revision <saved-sha256>`，检查完整建议图、精确类型替换、未改变的软件包 pin 和诊断。阻塞的预览不能提交。归属方 reader 和软件包 receipt 兼容后，运行 `isos config migrate --revision <saved-sha256> --proposed-revision <preview-sha256>`。任一修订已变时重新加载并预览。最终原子提交前，原始字节保持不变；迁移不会启动或重启进程。使用 `isos status --json` 检查新的保存修订号和旧的运行中修订号，确认就绪后再显式 Restart。参见[图迁移契约](../interfaces/node-graph_cn.md#g03-显式图迁移)和 [API](../interfaces/local-api_cn.md#g03-迁移路由)。

## Apply 前检查连线

对于新建或迁移后的 `0.1.0` 图，先运行 `isos config validate <draft.nodes.json>`，再运行 `isos config apply <draft.nodes.json> --revision <saved-sha256>`；首次 Apply 使用空修订。工作台 Validate API 仅在整图通过时返回 `edges`。检查每条连线的实际输入来源指针、路由类别、固定标志和 QoS 资格。只有精确的 EOS 相机 `frame` → 相机 gate `frame` 路由接受可选 `priority`（整数 `-20..19`，默认 `0`）或 `frequency_hz`（有限正数，单位 Hz；缺省表示不限频）。命令、健康、状态、结果或其他未路由连线均拒绝这些设置。`routed:true` 声明不表示实时健康。无效连线在 Validate、Apply、CLI 和迁移预览中显示精确指针及稳定代码；Run 和 Restart 在启动子进程前重新准入。修正草稿或恢复有效的保存修订后再次 Validate。参见[连线契约](../interfaces/node-graph_cn.md#g06-整图连线准入已实现契约)。

M1 中可以把 EGIA common 节点连线作为 `egia_local` 声明检查，但包含 EGIA 的 Apply 需要其归属方 setup 使用真实的 `eos_gateway` 端点。后续 reader-feature 与 gRPC 工作提供该网关；在完成配置和验证前，不要把 EGIA 纳入活动图。IsOS 会拒绝不完整的归属方 setup，且不会写入图。
