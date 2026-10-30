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

## 软件包节点贡献

任意 v4 软件包类型都可声明 `node_contributions`。`node_types` 和 `safety_adapters` 使用 EOS v1.0 注册表 JSON。IsOS 将其与精确的 `{id,version}` 软件包固定版本一起保存为收据旁的 `node-types.json`；EOS 通过可重复的 `--node-types` 参数读取。执行器节点类型必须具有支持停止的已注册安全适配器。链接包可运行 `isos pkg link ID VERSION BUNDLE --manifest release.json`；IsOS 校验并保存完整 v4 清单。没有清单的旧链接仍可使用，但不提供节点集合。

`node_sets` 对象包含 `id`、可选 `parameters` 和 `nodes`。每个节点包含符号 `ref`、`core`、`layer`、可选 `group` 和完整 `profile`。默认分组为软件包 ID 中 `/` 后的名称；MuJoCo 显式使用 `mujoco`。参数格式为 `{ "type": "string", "default": "可选" }`；所有 profile 字符串中的 `${name}` 都由传入值替换。`@ref` 会解析为完整图路径，适用于 `references`、`inputs` 和适配器设置。IsOS 注入 `profile.package={id,version}`，并拒绝冲突版本。Profile 字段遵循 [EOS 节点图契约](node-graph-v2_cn.md)。

安装软件包只会公开节点集合。`isos pkg node-sets` 用于列出集合，`isos pkg preview-set ID VERSION SET_ID graph.json --values values.json` 输出合成图和 `added_paths`。HTTP 预览行为相同。对已保存的旧 MuJoCo 图，`--replace-legacy-mujoco` 或预览请求中的 `replace_legacy_mujoco:true` 会在返回草稿中移除旧后端、硬件和依赖它们的控制节点，再加入新集合。应用仍通过带修订检查的图 API 完成。若已保存图中的任意节点引用了精确包版本，移除会被拒绝。
