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

发行归档可用 `isos pkg verify <manifest> <asset>` 检查平台、大小和 SHA-256，再用 `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` 安装本地候选包。正式发布后可用 `isos pkg install island/mujoco-sim --version 0.1.1` 安装已批准版本。`isos pkg list` 查看安装情况。Apply 和 Run 重新校验链接内容、可执行文件、所选场景及其资源、设备 ID 和 `MUJOCO_ROOT`。若已应用或运行中的图引用版本，`isos pkg remove island/mujoco-sim 0.1.1` 会拒绝删除。

参见[后端设计](../design/backend_cn.md)和[发行元数据](../interfaces/releases_cn.md)。

## P01 首次使用验收——目标实现

在 MuJoCo 仓库构建包，并把 v4 `release.json` 放在包旁。使用全新的 `ISOS_HOME`，对归档执行 `isos pkg verify release.json asset.tar.gz`，或对本地构建执行 `isos pkg link island/mujoco-sim 0.1.1 /absolute/bundle --manifest /absolute/bundle/release.json`。`isos pkg list` 应把精确版本显示为已验证并列出收据来源。单纯安装／链接不得改变初始图的保存修订、活动修订或进程状态。存在精确已验证固定版本后，才合成包节点集合、Validate、Apply、Run。[P01 收据契约](../interfaces/package-verification_cn.md)定义接受的文件与错误类别。

负面检查可复制已验证包至一次性目录，修改程序一个字节，或把资源目录替换为指向包外的符号链接，然后对固定该版本的图尝试 Apply 与 Run。两者都须在改变已保存图字节或替换活动子进程之前拒绝。历史无清单链接仍可列出，但其 `unverified_legacy_link` 状态会阻止两个图版本的 Apply 与 Run；须移除后使用 v4 清单重新链接。应恢复精确已验证发行，不要直接编辑收据或侧车文件。自动化用例及预期错误码见[P01 夹具](../../../tests/fixtures/package_verification/README.md)。
