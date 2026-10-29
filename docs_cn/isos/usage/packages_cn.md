# 软件包操作

IsOS 从 os-island.com 查询已批准的软件包，默认从 GitHub Releases 下载匹配平台的资产。安装时会校验平台、大小和 SHA-256。安装不会自动激活软件包；在 [部署图](../interfaces/deployment-graph_cn.md) 中选择准确的 ID 与版本。

    isos packages search mujoco
    isos packages info island/mujoco-sim --version 1.0.1
    isos packages install island/mujoco-sim --version 1.0.1
    isos packages list

开发工作区可用 isos packages link island/eos 2.0.0 /path/to/Island-eos 链接。未进入批准目录的候选版需要先执行 isos packages verify island-release.json asset.tar.gz，再使用 isos packages install --manifest island-release.json --asset asset.tar.gz --allow-unapproved 明确选择。

清单字段和激活模板见 [发行清单契约](../interfaces/releases_cn.md)。

按资源类型使用库命令查看部署节点、状态与安装包：

    isos services list
    isos services info vision
    isos services status vision --json
    isos backends install island/mujoco-sim --version 1.0.1
    isos skills install island/example-skill --version 1.0.0

install 在下载前校验发行包类型，激活仍由 isos config apply 完成。每个库命令也支持 remove <id> <version>；如果软件包仍被已选或运行中的部署引用，删除会被拒绝。

[English](../../../docs/isos/usage/packages.md)
