# R04 已验证 host 技能选择

[English](../../../docs/isos/interfaces/skill-selection.md) · [设计](../design/skill-selection_cn.md) · [首次使用](../usage/get-started_cn.md#r04-原生技能选择目标) · [固定用例](../../../tests/fixtures/skill_selection/README_cn.md)

**R04 所有者契约：IsOS 实现候选，仍待独立复核与跨所有者验收。** IsOS 拥有已保存图、P01 精确软件包 pin、P04 选中 generation 与 EOS 子进程启动输入。EOS 拥有 ABI v2 描述符准入、受限 worker、能力 Broker、Dispatcher 与 SkillService。安装软件包本身不启用技能。这里仅选择原生库，不定义图中的技能步骤、WorkflowService 或新的执行器通道。[EOS host ABI 契约](https://github.com/Island-Team/Island-eos/blob/refactor%28v2%29/runtime-main/docs_cn/eos/interfaces/skills/native/host-abi_cn.md)定义 guest 执行。

## 图中的选择

只有位于 `isos/process/component/eos`、`settings.role:"eos"` 且解析到 `aether-runtime` 的已选 `isos.process.component@v0.1` 节点可以有可选 `settings.skill_libraries`。`bridge`、`config` 和其他 EOS-role component 虽也有 `role:"eos"`，仍在其 `/settings/skill_libraries` 指针失败。G06 的 `@v0.1` component setting catalog 声明此字段；所有者检查进一步限定 runtime 节点。字段缺失或 `[]` 表示不启用外部 host，也不传 `EOS_SKILL_SELECTION_FILE`。非空条目只能是 `{ "package": { "id": "namespace/name", "version": "x.y.z" }, "library": "lib/island/skills/example.skill.so" }`；拒绝未知字段。`package` 是 P01 精确 pin；`library` 是软件包内 POSIX 相对路径，不是 glob、目录、命令或 URL。图不能指定 host 程序、worker 参数、handler、callback、步骤或特权能力。

| 字段 | 类型 / 单位 | 用途与校验 |
| --- | --- | --- |
| `settings.skill_libraries` | 数组 / 条目 | 可选，最多 16 项，仅限 EOS `@v0.1` component。生成文件按 `(package.id,package.version,library)` 排序；重复元组失败。保存图的字节和数组顺序仅在显式 Apply 时更新。 |
| `package.id` | 字符串 / 软件包 ID | 必填，规范 `namespace/name`，P01 精确验证。 |
| `package.version` | 字符串 / 发布版本 | 必填，P01 接受的精确版本，不使用 latest 或范围。 |
| `library` | 字符串 / POSIX 相对路径 | 必填 UTF-8，1–256 字节；分量不得为空、`.` 或 `..`，不得有反斜杠或 NUL，后缀为 `.skill.so`。目标必须是 1–64 MiB 的非符号链接普通文件；不扫描目录或递归发现。 |

P01 图 pin 收集器把每个选中 `package` 计入，并使用精确 `/cores/isos/layers/process/groups/component/nodes/<id>/settings/skill_libraries/<index>/package` 指针。IsOS 对每项重新取得 P01 `VerifiedPackage`，要求发布 `kind:"skill"`，检查 payload 根、文件身份与 SHA-256；变更的链接 payload 失败。只有明列的库被选中。EOS 随后检查所有库中重复的技能身份。旧 `1.0.0` 图和 `eos.control.skill@v1.0` 不因此获得执行行为。

对于选中的技能发布，IsOS 还检查打开的库文件、P01 `receipt.json` 与 `release.json`，以及已验证 payload 树中的每个普通文件。每个文件都必须由 IsOS 的有效 UID 拥有，且 group/other 均不可写（`mode & 0o022 == 0`）。选中库另需满足 1–64 MiB 限制；元数据与 payload 文件沿用 P01/EOS 大小限制。不安全文件在 Apply 发布或 Run/Restart 停止旧进程前失败；EOS 暂存前重复相同的所有者与权限检查。即使未选中的 payload 文件可被其他用户写入，整个技能包也会被拒绝。

G06 catalog 在 `isos.process.component@v0.1` 上以新增的 `data_type:"object_array"`、`min_items:0`、`max_items:16` 和 JSON Schema 对象 `items_schema` 如实描述该字段。item schema 为 `type:"object"`、`additionalProperties:false`，必需 `package` 与 `library`；`package` 也是不允许多余字段且需要字符串 `id`、`version` 的对象；`library` 是 1–256 字符的字符串。EOS registry 只接受有界的本地 schema 子集（`type`、`properties`、`required`、`additionalProperties`、`minLength`、`maxLength`、`pattern`），最多 8 KiB、深度 3，拒绝 `$ref` 与未知关键字。IsOS 进一步检查字节/路径、精确 owner 节点和 P01 pin。无新字段时，旧 setting metadata 序列化不变；`@v1.1` catalog 和旧图不变。下游 U01 表单获得真实的 object-array 描述，而非伪装成字符串。实现还需扩展 IsOS `ProcessSettings` 和 G06 聚合 catalog 适配器。

IsOS 拥有并发布同时含已有 `env_from` 和新 `skill_libraries` 的 `@v0.1` component 描述符，使用 EOS 新增的通用 `SettingType::ObjectArray` 支持。历史 EOS `@v1.1` catalog 描述符只是兼容输入，不拥有新的 IsOS 类型。旧 EOS 的 fallback clone 仅能为无技能选择的图提供 `env_from`，不得合成 `skill_libraries` 或准入非空选择。P05 仍检查选中 runtime 程序的精确 `host-skill-abi-v2` feature。

## 选中 generation 与交接

非空列表使 Apply 在不可变 P04 `v2/<active_revision>/<generation>/` leaf 中增加 IsOS 编写的 `isos/skill-selection.json`；schema-2 `manifest.json.files` 恰好列出该文件一次，`manifest.json.packages` 列出相关 pin。文件为 `0600`，父目录为 `0700`。紧凑、排序后的 JSON 字段恰好为 `schema_version:1`、`active_revision` 和 `libraries`。每个条目恰好含 `package_id`、`package_version`、`package_root`、`payload_root`、`receipt_sha256`、`release_sha256`、`payload_sha256`、`library` 与 `library_sha256`；所有摘要为 64 位小写十六进制 SHA-256。`package_root` 是 P01 私有元数据 store 的绝对路径；`payload_root` 是 P01 已验证 payload 的绝对路径，链接包可位于外部。它们只能由 IsOS 的已验证句柄产生，不能由图或浏览器提供。

完整序列化文件最多 65,536 字节（64 KiB），与 EOS 的有界选择读取器一致。已验证链接包路径过长等情况导致文件超限时，IsOS 在 Apply 发布或 Run/Restart 替换前于 EOS 的 `settings.skill_libraries` 指针拒绝。

`active_revision` 等于保存图字节及 P04 manifest 的 `revision`，不同于 EOS projection 摘要。文件不含 `generation`，因为 P04 generation 本身包含最终文件哈希；leaf 路径和 manifest 提供 generation。空列表不生成该文件，选中 leaf 中意外出现该文件应失败。只有列表非空时，选中的 `aether-runtime --capabilities` 必须声明精确 `host-skill-abi-v2`；缺失则在 Apply、Run/Restart 停止旧子进程或执行 guest 前返回 P05 `reader_feature_unsupported`。IsOS 只向选中的 EOS 子进程注入 `EOS_SKILL_SELECTION_FILE=<选中文件绝对路径>`，其他子进程清除继承值；作者设置的冲突值或保留参数在停止旧运行前失败。R03 的角色 grant 独立存在。

Run/Restart 在停止旧子进程之前重新检查保存图、V2 selection 与 marker、完整 manifest、P01 pin、库摘要和 EOS feature。缺失、变更、未列出、不安全或 revision 错误的选择失败，并保留当前 PID、run ID、active revision 与 grant。G03 和旧 flat 快照不能启用该选择。相同图 revision 的包或二进制变更需要再次 Apply 来选择新 generation，再显式 Restart 才生效。自动 EOS 子进程重启沿用同一次运行的文件和 grant；Stop 在子进程回收后撤销 R03 grant。

EOS 在 worker 查询前独立验证 manifest、文件、revision、摘要和路径。对于链接包，EOS 以 nofollow 打开精确文件，从同一打开句柄计算摘要并复制到 EOS 私有只读暂存文件；受限 worker 只加载该副本。无效 ABI、描述符或不支持的已声明 callback 类别在 handler 调用前失败；handler 运行时请求未声明 primitive 则由 Broker 在 Dispatcher release 前拒绝。IsOS 与 EOS 的两次检查均不可省略。

P01 损坏沿用 `package_changed`、`package_unsafe_path` 或 `package_missing_file`，指向条目 `/package`；P04 快照损坏沿用 `generated_manifest_mismatch`；P05 feature 缺失沿用 `reader_feature_unsupported`。R04 语义错误使用现有 schema-1 API 诊断 `{code,path,message,stage}`：`skill_selection_invalid`（`graph`）、`skill_package_incompatible`（`package`）、`skill_library_unsafe`（`package`）、`skill_library_changed`（`package`）。消息提供修复动作，不包含原始路径、库内容、grant 或 worker stderr。固定用例覆盖空 host、显式选择、重复、路径、符号链接、变更的链接包、旧 EOS feature、错误 generation、旧 PID 不变、环境隔离、无效 ABI、未声明能力及 worker 回收。完整输出保存于 `logs/test/<run-id>/`。
