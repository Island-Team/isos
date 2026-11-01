# 已验证软件包准入

[English](../../../docs/isos/interfaces/package-verification.md)

**P01 归属方契约——已实现。** IsOS 负责安装、收据、发行快照及包准入；CLI、本地 API 与图准备调用同一校验器。EOS 只接收精确已验证固定版本解析出的路径，不自行安装或信任侧车文件。[v4 发行封装](releases_cn.md)定义分发字段；本契约定义本地验证边界。MuJoCo 场景／设备语义属于 P02，选择器选项属于 P03。

## 调用与结果

`isos pkg install ID --version VERSION`、`isos pkg install --manifest RELEASE --asset ARCHIVE --allow-unapproved` 与 `isos pkg link ID VERSION BUNDLE --manifest RELEASE` 选择精确 `{id,version}`。等价本地接口为 `POST /api/isos/v1/packages/install` 和 `/link`；`isos pkg verify RELEASE ARCHIVE` 只检查而不发布。获批准目录的响应仍须按 v4 元数据解析和验证。本地候选需要 `--allow-unapproved`。未提供 `--manifest` 的链接照旧发现 `<bundle>/release.json`；其中有效的 v4 清单会生成已验证链接。只有既无显式清单又无发现的 v4 清单时，才以 `unverified_legacy_link` 状态列出，供检查和移除，但无论图版本如何，都不得用于 Apply、迁移提交、Run 或 Restart。已保存旧图字节仍可读取且保持不变。所有命令均不改已保存图字节、保存修订、活动修订或子进程。

成功时 IsOS 原子发布一个私有的 `data/packages/<namespace>/<name>/<version>/` 目录。归档安装包含复制的载荷；链接包含校验器拥有的元数据和规范化外部来源路径。链接来源仍可变化，因此 Apply 和 Run 每次重新计算冻结的指纹；任何操作不得暗中刷新收据。替换精确版本须在保存图和活动图都不引用它后显式移除，再重新安装／链接。失败要清理暂存区，不发布部分收据或侧车，并保持图及进程状态不变。

收据是带版本的 JSON 对象，公开字段如下：

| 字段 | 类型／单位 | 用途、默认值及校验 | 兼容性 |
| --- | --- | --- | --- |
| `schema_version` | 整数／— | P01 已验证收据必为 `2`。 | 旧 `1` 可读取，但重新安装／链接前视为未验证。 |
| `id`, `version` | 字符串／— | 必须与 v4 发行及库键完全一致；拒绝版本路径分隔符和非规范包 ID。 | 不推断最新版本。 |
| `source_kind` | `approved_catalog`、`local_candidate` 或 `local_link`／— | 记录接受的元数据与载荷来源。 | 缺失该字段的旧收据未验证。 |
| `source_commit` | 40 位十六进制字符串／Git 提交 | 必须来自 v4 清单；只表示来源，不单独证明已安装字节。 | 必须与冻结发行一致。 |
| `asset_sha256` | 64 位十六进制字符串或 null／SHA-256 | 归档必需，匹配选定平台资产；仅链接可为 null。 | 链接使用 `payload_sha256`。 |
| `release_sha256` | 64 位十六进制字符串／SHA-256 | 校验器拥有的规范发行快照摘要。 | Apply／Run 重算。 |
| `payload_sha256` | 64 位十六进制字符串／SHA-256 | 已验证载荷文件树摘要，包含描述文件、程序和资源。 | Apply／Run 从安装或链接根目录重算。 |
| `sidecar_sha256` | 64 位十六进制字符串或 null／SHA-256 | 规范 `node-types.json` 投影摘要；仅无贡献时为 null。 | 加载前重算并与发行投影比较。 |
| `local_link` | 绝对路径或 null／— | 链接的规范化来源根目录；归档安装为 null。 | 路径本身不是执行授权。 |

载荷摘要为 SHA-256：先写入字节 `isos-package-tree-v1\0`，再按 UTF-8 包相对路径排序写入每个项。每项依次写入小端 `u32` 路径字节长度、路径字节、一个类型字节（普通文件 `F` 或目录 `D`）、一个可执行位字节（`0` 或 `1`）、小端 `u64` 长度（目录为 `0`）及 32 个原始文件内容 SHA-256 字节（目录全为零）。摘要只纳入文件执行位；另行检查仅所有者访问的存储权限。拒绝符号链接、特殊文件、规范化后重复或逃逸的路径。归档不得包含顶层保留的校验器元数据名称。归档安装的载荷树仅排除校验器写入库内的 `receipt.json`、`release.json`、`node-types.json`；发行和侧车使用上表的独立摘要。链接载荷树纳入来源根目录内所有文件，包括存在的来源 `release.json` 或 `node-types.json`；校验器拥有的副本位于库内并单独复查。来源外部的显式清单会复制进库；之后修改原外部文件无关，因为接受的快照才是授权依据。文件数量和总字节受归档上限约束，使用流式读取。规范清单和侧车序列化必须确定，发布与复查共用。能同时替换所有归属方文件的本机管理员不在收据信任声明范围；收据用于发现意外或部分篡改，并把全部内容绑定到精确发行固定版本。

发布前，两种来源均检查所选资产的主机 OS、架构及声明的运行时选择条件。归档安装额外对照所选资产检查归档的精确字节大小与 SHA-256；链接没有归档字节，记录 `asset_sha256:null`。只解压到私有暂存目录，拒绝绝对路径、`..`、链接、设备和重复项。写侧车前验证 v4 发行身份、类型、精确 `@v0.1` 或明确支持的历史 `@v1.0` 贡献 ID、唯一贡献及安全适配器，并检查声明的描述文件／入口引用。`island/mujoco-sim`（或声明 MuJoCo 仿真器契约的发行）须包含有大小限制、`{id,version}` 匹配、schema／协议受支持的 `backend-v2.json`；入口须是包内有执行位的普通文件；场景及其引用资源须是包内普通文件。其他后端发行可仅有适配器库；除非主动声明描述文件，否则不要求 MuJoCo 描述文件。声明的适配器库须在包内并匹配自身 SHA-256。引用路径的每个组成部分都不得是符号链接，变化后的链接来源也一样；仅规范化前缀检查不足。P01 检查存在性、边界和内容完整性；P02 检查 MuJoCo 专用场景 ID、设备映射、库可用性及资源显示说明。

Apply 与 Run 在归属方 setup、生成快照发布或子进程启动前重新检查收据 schema／固定版本、发行摘要与来源提交、所选平台元数据、侧车与发行投影、载荷树摘要、描述文件及全部引用路径。来源载荷、安装载荷或校验器拥有的发行快照变化，在 `payload` 或 `manifest` 阶段返回 `package_changed`；侧车不匹配在 `sidecar` 阶段返回 `package_invalid_sidecar`；新增符号链接或路径逃逸在所在阶段返回 `package_unsafe_path`。重新使用前须在检查引用后显式重新安装／链接。图只声明绑定；包贡献节点不能跳过已注册位置或调度器准入来授权执行器路线或程序启动。

固定版本收集器覆盖 `node.package`、`node.adapter.package` 及历史成对的 `node.settings.package` 与 `node.settings.version`。软件包复查、归属方 setup 的 `packages`、侧车选择、Apply、Run 和 Restart 共用规范化的 `{id,version}` 集；缺失或不完整的固定版本在节点路径失败。`node_sets::list/compose`、后端发现、软件包列表及链接预览也消费已验证句柄：损坏元数据不能成为就绪目录项或合成节点集合预览。发现接口可将其显示为不可用并附清理过的原因。CLI 与 HTTP 列表行包含 `status`（`verified`、`unverified_legacy_link` 或 `unavailable`）、布尔值 `available` 和清理过的 `reason`；已验证行保留 `source_kind` 来源。只有候选图或已保存图实际引用的固定版本参加准入。无关的已安装包可在发现列表中显示不可用原因，但不能向该图注入侧车，也不能阻断其 Apply／Run。运行时目录发现不得把全库扫描到的任意侧车文件视为已验证授权。

CLI 失败返回非零状态。HTTP 保留现有 schema-1 `{schema_version,ok,data,error}` 封装：`error` 是清理过的字符串，P01 失败时 `data.diagnostics` 增加有界分类数组。校验／准入失败为 `422`，精确版本冲突或有引用的移除为 `409`。诊断对象必含 `code`（字符串／—）、`path`（JSON 指针或空字符串／—）、`message`（字符串／—）、`id` 和 `version`（精确字符串／—）及 `stage`（有界字符串／—），不添加任意字段。`code` 为 `package_platform_mismatch`、`package_digest_mismatch`、`package_invalid_manifest`、`package_unsafe_path`、`package_missing_file`、`package_invalid_descriptor`、`package_invalid_sidecar`、`package_changed` 或 `package_version_exists`。`stage` 仅可为 `manifest`、`asset`、`extract`、`descriptor`、`payload`、`sidecar` 或 `admission`。Apply／Run 的 `path` 指向图固定版本；直接包操作时为空。消息说明精确固定版本和修复动作，不暴露本机来源路径、下载 stderr、密钥、原始清单或任意环境内容。包日志包含错误码、精确身份、来源类别、可用摘要／提交来源与管理器会话关联 ID，不写原始错误链。

例如，`POST /api/isos/v1/packages/link` 接收 `{ "id": "island/mujoco-sim", "version": "0.1.1", "path": "/home/user/bundle", "manifest": "/home/user/bundle/release.json" }`。成功响应保留现有 `data.path` 与 `data.installed`，增加 `data.verified:true` 与 `data.source_kind:"local_link"`；`installed` 包含该精确固定版本的新 schema-2 收据。链接程序变化示例：`{ "schema_version": 1, "ok": false, "data": { "diagnostics": [{ "code": "package_changed", "path": "/cores/eos/layers/process/groups/mujoco/nodes/sim/package", "message": "Reinstall or relink island/mujoco-sim@0.1.1", "id": "island/mujoco-sim", "version": "0.1.1", "stage": "payload" }] }, "error": "Reinstall or relink island/mujoco-sim@0.1.1" }`；Apply 与 Run 均保持保存和活动修订不变。

[P01 夹具](../../../tests/fixtures/package_verification/README.md)冻结正反例；首次使用命令见[软件包操作](../usage/packages_cn.md)。P04／P05 在生成配置及启动读取器之前复用本准入。
