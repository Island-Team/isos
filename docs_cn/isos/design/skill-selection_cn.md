# R04 技能选择事务

[English](../../../docs/isos/design/skill-selection.md) · [所有者契约](../interfaces/skill-selection_cn.md) · [生成配置](../interfaces/configuration_cn.md) · [软件包验证](../interfaces/package-verification_cn.md)

**设计阶段。** R04 扩展已实现的 P01/P04/P05 与 R03 边界。图在 EOS component 上选择有界的精确原生库；IsOS 不解释 ABI 描述符，也不执行技能代码。EOS host ABI 与 worker 契约负责这些行为。这里不新增图技能节点、步骤执行器、ActionList 或第二条执行器路径。

Apply 首先验证保存的 `0.1.0` 图与精确 EOS component 位置；按拒绝未知字段的结构解析最多 16 个 `skill_libraries` 项，在精确数组索引 JSON Pointer 上拒绝重复元组、错误 pin 和不安全相对路径。旧图读取器不投影此设置。然后将每项加入 P01 图 pin 集合；每个精确包重新验证一次，要求 `kind:"skill"`，以 nofollow 检查库路径分量、普通文件、1–64 MiB 上限及 SHA-256。仅使用已验证句柄，不扫描 store 或递归搜索 `*.skill.so`。同包可显式选择多库。

若列表非空，探测选中 EOS runtime 的 `host-skill-abi-v2`。从已验证句柄按顺序生成 `isos/skill-selection.json`，把摘要加入 P04 的 `files`，把收据加入 `packages`，并使用原 P04 规范 manifest 算法计算 generation。发布前重检文件身份和摘要。包或二进制变更需要重新 Apply，即使图字节未变。发布沿用 P04 的 fsync、selection-map、marker 与图 rename 次序；不确定的 rename 只能通过重新读取已观察到的图 revision、map、marker 和完整 leaf 来判断。

IsOS 的 G06 聚合 catalog 发布由 IsOS 拥有的精确 `isos.process.component@v0.1` 描述符，包含 `env_from` 与结构化 `skill_libraries`；EOS 只提供通用 `SettingType::ObjectArray` 和有界 schema 支持。旧 EOS fallback clone 仅能让无技能选择的图使用 `env_from`，不能声明或准入非空选择；历史 `@v1.1` 描述符不变。

Run/Restart 在 deployment lock 下执行现有 G01/G06/P01/P04/P05 与 R03 预检，要求非空选择必须指向明确的 schema-2 V2 leaf，重新计算图、文件、软件包与库摘要，将新渲染的选择字节和 manifest 文件逐字节比较，再检查全部待启动进程并解析 secret。只有预检通过才分配新的 R03 run，并只向 EOS 子进程传 `EOS_SKILL_SELECTION_FILE`。config、bridge、EGIA、MuJoCo 与独立 service 均不得接收。预检失败保留旧 PID 和 grant。自动 EOS 重启沿用同一文件；显式 Restart 再检查并轮换 R03 run/grant。失败的候选运行按现有恢复路径处理，并使用新 run ID。

EOS 在目录变成可用前再次检查选中 manifest 与库内容。链接 payload 可能在 IsOS 预检与 EOS 启动之间变化；EOS 以 nofollow 打开同一文件句柄，哈希并复制到私有暂存，受限 worker 只加载该副本。Catalog 准入不能调用 handler；无效 ABI/描述符/能力声明在调用前失败。此负面边界不宣称能隔离任意同 UID 或内核受损的软件包代码。

验证使用[机器用例](../../../tests/fixtures/skill_selection/README_cn.md)。单元测试检查指针、pin 收集、排序与去重、规范字节、P01 链接变化、P04 generation 和子进程环境。stub 集成测试启动两代进程：篡改库和旧 EOS feature 在停止第一代前拒绝；有效选择只到达 EOS。EOS owner replay 检查错误 revision、ABI/描述符、未声明能力和 worker 回收。完整命令与输出放入 `logs/test/<run-id>/`；实现后在此记录结果。

设计用例检查（2026-11-02）：`python3 tests/fixtures/skill_selection/check_cases.py` 通过 19 个用例 ID 与选择文件结构；`git diff --check` 通过。变更文档的相对链接扫描未发现缺失目标。完整本地输出在忽略的 `logs/test/20261102-r04-isos-design/`。这只检查契约结构；实现与跨所有者重放仍未完成。
