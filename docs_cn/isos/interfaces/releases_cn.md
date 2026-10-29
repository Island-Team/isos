# 发行清单契约

IsOS 默认从 os-island.com 获取已批准的发行元数据，从 GitHub Releases 下载资产。候选发行也可以通过 --allow-unapproved 明确安装。发行文件名为 island-release.json，schema_version 为 4。

GitHub Release 资产上传完成后，软件包开发者可运行 `isos packages publish --manifest island-release.json` 发布完整的 v4 清单。该命令只提交目录元数据，不会上传 GitHub Release 资产。详见[身份验证](authentication_cn.md)和[软件包操作](../usage/packages_cn.md)。

[English](../../../docs/isos/interfaces/releases.md)

清单包含 id、kind、version、source_commit、contracts 和 assets。kind 可为 component、service、backend、skill、extension、example。IsOS 解析器允许省略 source_url、release_url、description、license 和 activation；已批准的目录在提交审核时要求 source_url 与 release_url。资产包含 os、arch、url、size_bytes、sha256；可选字段包括 name、libc、gpu、ros、required_libraries 和 tested_ubuntu。

    {
      "schema_version": 4,
      "id": "island/mujoco-sim",
      "kind": "backend",
      "version": "1.0.1",
      "source_commit": "0000000000000000000000000000000000000000",
      "contracts": ["eos.simulator.v1"],
      "assets": [{
        "os": "linux", "arch": "x86_64", "libc": "glibc>=2.35",
        "url": "https://github.com/Island-Team/isos-mujoco-sim/releases/download/v1.0.1/asset.tar.gz",
        "size_bytes": 123456,
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
      }]
    }

示例大小和哈希仅为占位值。安装时根据平台和可选条件选择资产，校验大小与 SHA-256；压缩包只允许安全相对路径中的常规文件和目录。失败时命令返回非零状态并说明不匹配的字段，例如 no asset for linux x86_64。

activation.files 可指定 TOML 模板的 source 和 destination。模板里的 {{package_file:relative/path}} 在 TOML 校验前替换为软件包内文件的绝对路径。安装只写入软件包库；激活发生在部署图 apply 阶段，不会自动启动服务。

相关：[部署图](deployment-graph_cn.md)、[软件包操作](../usage/packages_cn.md)。
