# Aether 后端软件包

[English](../../../docs/isos/usage/packages.md)

`island/mujoco-sim@0.1.1` 是首个后端包，包含可执行文件、场景、资源、节点集合和 `backend-v2.json`。v4 发行清单只描述分发，独立于 v1 图和节点类型。安装或链接不会自动激活。`isos init` 创建不含硬件的通用草稿。先预览已安装的节点集合并选择场景，再 Apply 图供 Run 使用。

本地开发时，在 MuJoCo 仓库构建仿真器，并用 `python3 tools/build-backend-bundle.py --binary <simulator> --output <new-directory>` 制作包，再执行：

```sh
isos init
isos pkg link island/mujoco-sim 0.1.1 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
# 在工作台预览 sim-hardware；也可使用 isos pkg preview-set 和参数文件。
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

发行归档可用 `isos pkg verify <manifest> <asset>` 检查平台、大小和 SHA-256，再用 `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` 安装本地候选包。正式发布后可用 `isos pkg install island/mujoco-sim --version 0.1.1` 安装已批准版本。`isos pkg list` 显示已验证来源，并将旧链接或损坏包标为不可用。Apply 和 Run 重新校验链接内容、可执行文件、所选场景及其资源、设备 ID 和 `MUJOCO_ROOT`。若已应用或运行中的图引用版本，`isos pkg remove island/mujoco-sim 0.1.1` 会拒绝删除。

参见[后端设计](../design/backend_cn.md)和[发行元数据](../interfaces/releases_cn.md)。

## P02 场景与设备目录检查

`0.1.0` 图须使用 `backend-v2.json` 含 `catalog_version:"0.1.0"` 的包，包含带 profile／SHA-256 的 `pick-place`、`navigation` 两个场景及完整相机／底盘／机械臂设备目录。完全不含 P02 字段的历史描述文件只供旧图使用，不能提供新预设。链接或安装精确已验证的 `island/mujoco-sim@0.1.1` 发行版，将 `MUJOCO_ROOT` 指向匹配的 MuJoCo 3.16.0 环境，再用场景 ID、匹配的 `camera_device`、`base_device`、`manipulator_device` 及仅所有者可访问的 `socket` 预览 `sim-hardware`。起始图已有 Dispatcher；预设加入 EOS 通用硬件、原语绑定和包贡献的进程节点，不加入 Vision 节点。见[IsOS 目录契约](../interfaces/mujoco-catalog_cn.md)与[固定负例](../../../tests/fixtures/scene_catalog/README.md)。

预览仅改变草稿。Validate 和 Apply 在发布新保存修订前复查精确收据、场景 profile／XML 摘要及引用资源、设备类别与场景适用性和主机库。Run 和 Restart 在替换子进程前重复检查。修改场景须按修订 Apply 后 Restart。场景／设备／库缺失或已链接资源变化，均不得修改保存字节或活动子进程。[P03 节点选项](../interfaces/node-options_cn.md)通过通用 API 展示这些已验证 ID；P02 检查不依赖该 UI。

### P03 选择流程（目标）

预览 `sim-hardware` 后，把该**草稿**以 `{"schema_version":1,"graph":<草稿>}` 发送到 `POST /api/isos/v1/node-options`。从 `data.setting_options` 读取 `eos/process/mujoco/simulator` 的 `scene` 设置，以及每个通用相机／底盘／机械臂节点的 `adapter.settings.device_id`、`adapter.settings.simulator_node`。值为稳定 ID；名称与说明只供展示。选择一个可用场景和各类别对该场景适用的设备，同时写入每个适配器的两个设备字段。修改场景、软件包固定版本、backend 引用或节点类型后重新获取选项。使用 GET 时，将 `data.graph_revision` 与 `GET /node-graph` 的修订号比较，不同则重新读取；使用 POST 时只接受当前草稿最后一次请求的响应。不可用的已保存 ID 仍显示 `reason_code` 和修复提示；应选择已验证选项或重新链接精确发行版本。随后对最新保存图修订执行 Validate 和 Apply。修订过期返回 409；已链接字节变化、错误固定版本、运行库缺失或所选 ID 不再适用时返回精确 422 诊断，不改变保存字节和活动子进程。精确路径见[响应与负例夹具](../../../tests/fixtures/node_options/README.md)。

## P01 首次使用验收

在 MuJoCo 仓库构建包，并把 v4 `release.json` 放在包旁。使用全新的 `ISOS_HOME`，对归档执行 `isos pkg verify release.json asset.tar.gz`，或对本地构建执行 `isos pkg link island/mujoco-sim 0.1.1 /absolute/bundle --manifest /absolute/bundle/release.json`。`isos pkg list` 应把精确版本显示为已验证并列出收据来源。单纯安装／链接不得改变初始图的保存修订、活动修订或进程状态。存在精确已验证固定版本后，才合成包节点集合、Validate、Apply、Run。[P01 收据契约](../interfaces/package-verification_cn.md)定义接受的文件与错误类别。

负面检查可复制已验证包至一次性目录，修改程序一个字节，或把资源目录替换为指向包外的符号链接，然后对固定该版本的图尝试 Apply 与 Run。两者都须在改变已保存图字节或替换活动子进程之前拒绝。历史无清单链接仍可列出，但其 `unverified_legacy_link` 状态会阻止两个图版本的 Apply 与 Run；须移除后使用 v4 清单重新链接。应恢复精确已验证发行，不要直接编辑收据或侧车文件。自动化用例及预期错误码见[P01 夹具](../../../tests/fixtures/package_verification/README.md)。
