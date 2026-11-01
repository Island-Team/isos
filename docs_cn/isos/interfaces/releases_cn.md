# 发行清单契约

IsOS 默认从 os-island.com 获取已批准的发行元数据，从 GitHub Releases 下载资产。候选发行也可以通过 --allow-unapproved 明确安装。发行文件名为 island-release.json，schema_version 为 4。

P01 本地完整性与收据边界见[已验证软件包准入](package-verification_cn.md)。下方历史 v4 示例说明发行封装；已验证的 `0.x` Apply 和 Run 需要 P01 收据并复查精确固定版本。

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

相关：[部署图](node-graph_cn.md)、[软件包操作](../usage/packages_cn.md)。

## 软件包节点贡献

上方 `1.0.1` 资产片段仅演示发行清单结构，属于历史示例。v2 MuJoCo 节点包使用 `island/mujoco-sim@0.1.1` 和下列贡献字段。

任意 v4 软件包类型都可声明 `node_contributions`。`node_types` 和 `safety_adapters` 使用 EOS v1.0 注册表 JSON；`data_types` 声明软件包专用数据 IDL。IsOS 将这些数组与精确的 `{id,version}` 软件包固定版本一起保存为收据旁的 `node-types.json`；EOS 通过可重复的 `--node-types` 参数读取。每个数据类型必含 `id`（带 `@vM.m` 版本的 `namespace.name` 字符串）、`version`（与 ID 后缀一致的 `M.m` 字符串）、`purpose`（非空、供人阅读的字符串）、`media_type`（非空 MIME 字符串）、`codec`（非空且已注册的运行时编解码器 ID，例如 `json_v1`）、`schema`（JSON Schema 对象）和 `max_payload_bytes`（正整数，单位字节）。EOS 运行时仅接纳已编译并注册编解码器的数据类型。执行器节点类型必须具有支持停止的已注册安全适配器。链接包可运行 `isos pkg link ID VERSION BUNDLE --manifest release.json`；IsOS 校验并保存完整 v4 清单。没有清单的旧链接仍可使用，但不提供节点集合。

`node_sets` 对象包含 `id`、可选 `parameters` 和 `nodes`。每个节点包含符号 `ref`、`core`、`layer`、可选 `group` 和完整 `profile`。默认分组为软件包 ID 中 `/` 后的名称；MuJoCo 显式使用 `mujoco`。参数格式为 `{ "type": "string", "default": "可选" }`；所有 profile 字符串中的 `${name}` 都由传入值替换。`@ref` 会解析为完整图路径，适用于 `references`、`inputs` 和适配器设置。IsOS 仅对该软件包贡献的节点类型注入 `profile.package={id,version}`，并拒绝冲突版本。集合内置的 EOS 和 IsosUI 节点保持无包固定版本。Profile 字段遵循 [EOS 节点图契约](node-graph_cn.md)。

安装软件包只会公开节点集合。`isos pkg node-sets` 用于列出集合，`isos pkg preview-set ID VERSION SET_ID graph.json --values values.json` 输出合成图和 `added_paths`。HTTP 预览行为相同。应用仍通过带修订检查的图 API 完成。若已保存图中的任意节点引用了精确包版本，移除会被拒绝。

软件包可选的 `node_contributions.adapter_families` 是适配器实现描述数组。每项必须含 `id`（非空、在该发行版内唯一的稳定字符串）、`device_kinds`（非空设备类别字符串数组，例如 `camera`、`base`、`manipulator`）、`transport`（非空传输方式字符串，例如 `unix_socket`）和 `description`（非空、供人阅读的用途）。可选字段为 `supported_primitives`（精确 `name@version` 字符串数组）及 `settings_schema`（适配器设置的 JSON Schema 对象）。IsOS 在已验证 sidecar 中保留软件包来源。EOS 在目录中计算 `compiled` 与不可用时的 `reason`，软件包不能自行声称适配器已编译。通用硬件节点类型与适配器身份独立。

## 0.x 目标：已验证选项与包专用节点——尚未完整实现

v4 发行清单继续作为分发封装。其 `schema_version`、`id`、`kind`、`version`、`source_commit`、`source_url`、`release_url`、`contracts`、`assets` 与可选 `activation` 保持上文含义；`1.0.1` 示例是历史示例，而非本次发行目标。获批注册表条目提供元数据和资源 URL；IsOS 下载精确资源，验证主机平台选择、长度与 SHA-256，只解压安全的普通路径，并写入收据；随后把贡献类型／数据／适配器声明与精确包版本一起快照。节点集合只是模板；必须合成草稿，再通过修订检查 Apply。包不能仅凭贡献节点就获得任意命令执行或执行器路由权限。

目标包描述文件增加可选资源索引，供设置选择器使用；MuJoCo 首先沿用 `backend-v2.json` 中已有场景和设备 ID。未来资源索引遵循下表，在已验证包目录内检查文件。描述文件名是包兼容身份；文档文件名清理不直接改名。

| 资源字段 | JSON 类型／单位 | 用途和所有者 | 默认值与校验 | 兼容 |
| --- | --- | --- | --- | --- |
| `kind` | 字符串／资源家族 | 包声明 `scene`、`device` 或以后注册的家族 | 必需已知家族 | 新家族需解析器 |
| `id` | 字符串／稳定选择值 | 图设置使用的包身份 | 必需，同发行版本同种类内唯一；非空且无路径分隔符 | 旧图 ID 保持有效 |
| `label` | 字符串／— | 操作员显示名 | 新资源非空；旧场景／设备描述默认用 ID | 显示名可改 |
| `description` | 字符串／— | 功能和用法 | 新资源非空；旧描述在目录中生成回退 | 不改写图 |
| `path` | 相对包路径或 null／— | 文件资源字节 | 场景／文件须是安全普通路径；逻辑设备为 null | 不存入图，不发给 UI |
| `sha256` | 64 位十六进制或 null／摘要 | 发现资源变化 | 新文件资源必需；旧 MuJoCo 描述使用已验证包指纹 | Apply／Run 复查 |

注册设置 `Setting.selector={provider:"package",resource:"scene",package_from:"node.package"}` 时，IsOS 通过 [node-options](local-api_cn.md#发行目标设置选项尚未实现)返回包内稳定场景 ID。若发行缺失、未验证、变化或与 EOS 已编译适配器不兼容，选项仍可见但附不可用原因，Apply 失败。包元数据与有类型硬件负载分离；EOS 内部适配器注册表判断 `mujoco-v2` 能否连接。新的 `@v0.1` 进程类型由 MuJoCo 包贡献；通用相机／底盘／机械臂类型仍归 EOS。
