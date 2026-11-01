# R03 启动授权交接设计

[English](../../../docs/isos/design/control-grants.md) · [所有者契约](../interfaces/control-grants_cn.md) · [用例](../../../tests/fixtures/control_grants/README_cn.md)

**EOS/IsOS 本地 R03 重放已通过；已审查 PR 的合并仍待完成。** R03 使 IsOS 成为 EOS 托管 Dispatcher 权限的签发方。签发属于管理器的 Run/Restart 事务，不属于 Apply 或不可变的 P04 生成代。已保存图确定精确修订和接收方，但不含凭据字段。所选 P04 叶目录提供已验证的 EOS 投影；独立的私有运行时授权包绑定运行及其修订。

## 事务

1. 在部署锁下重读保存字节，要求精确修订有显式 schema-2 `V2` 选择及匹配的 manifest。仅调用 `generated_v2::selected_root` 不够：未选中的 `0.1.0` 图也可能获得 G03 回退路径。缺失选择或 G03 选择必须在签发授权或停止子进程之前被拒绝。随后验证精确软件包 pin、所有者渲染、已选二进制、读取器特性及所有 `env_from` 值。现有 P04 profile 投影要求托管 `0.1.0` 执行器运行具备已选 EOS 和 config；config 不接收授权，EGIA 与 bridge 可选，但任何已选接收方都必须有匹配的已验证进程规格和启动输入。候选计划遗漏必需的 EOS 或已声明的已选接收方、代变化或图作者写入的所有者保留启动标志，都必须在停止活动子进程前被拒绝。候选 `active_revision` 使用最终保存字节的 SHA-256。
2. 仅当 Start 创建运行或 Restart 替换运行时分配新的部署 `run_id`。通过操作系统 CSPRNG 独立生成两个授权。使用 no-follow/exclusive 创建全新私有运行目录及三个文件，严格权限和大小，fsync 文件及目录。验证文件绑定运行 ID 与修订。不得将授权包加入 `manifest.json` 或代摘要；相同修订的 Restart 也应轮换授权，且不改变 Apply 输出。
3. 授权包持久化后，IsOS 才可停止旧进程。按依赖顺序启动候选子进程，把验证文件路径传给已选 EOS、EGIA 授权路径传给已选 EGIA、操作员授权路径传给已选 bridge。先从继承环境和图作者设置的子进程环境中移除三个保留变量。未选进程不获授权。停止前，拒绝已选 EOS runtime/bridge/config `settings.args` 中图作者写入的 `--run-id` 或 `--run-id=...`，以及所有者注入的 backend 摘要和 log-dir 标志；现有生成 profile 预检拒绝重复的 `--profile`。EOS 的有效 IsOS `--run-id` 及 EGIA 的 `ISOS_RUN_ID` 必须匹配验证文件；子进程不能自行选择另一个图或运行。
4. 等待超过 TCP 绑定的 EOS 所有者就绪：manifest 与授权已验证、发现完整、写入准入状态正确。已选择的 EGIA 和 bridge 就绪依赖其被允许的 EOS 路径。整个依赖链通过后才发布 `deployment_ready`。`deployment_starting` 前的授权拒绝以管理器会话 ID 和请求关联号发出 `deployment_preflight_failed`，不改变旧 P07 当前运行范围。`deployment_starting` 后的子进程故障属于候选运行：先停止依赖方，再停止提供方，发出其终结故障事件，回收进程后撤销授权包。
5. 显式 Restart 时，旧子进程停止并回收后才撤销旧授权包。替换失败后若恢复旧进程规格，必须使用**新**运行 ID 和授权包；若恢复授权签发或交付失败，部署保持 degraded。不得以旧运行凭据重启旧规格。同一活动运行中的自动子进程重启保留授权包，并在重用前验证其常规文件，便于查询丢失响应的操作。Stop 在回收全部接收方后撤销授权包。管理器进程重启不得为新运行选择孤立的旧授权包；只有确认相关子进程已退出后才能清理。

运行授权包是权限能力；祖先符号链接、错误 UID、宽松权限、特殊文件、目标冲突、截断十六进制、缺少换行、额外字段、摘要变化、运行或修订不匹配都必须在就绪前失败。文件描述符和路径读取执行 no-follow 检查；接收方边界再次检查权限。停止旧进程前准备的候选授权包不会附着到旧进程。清理错误以脱敏本地诊断报告，不能使未验证子进程变为 ready。文件内容、授权摘要、bearer 元数据和运动实参均不得进入事件、状态或图错误体。

已选 EOS `--capabilities` 探测必须声明精确的 `managed-control-grants`，IsOS 才能签发授权包或停止旧子进程。这道 P05 跨版本门槛拒绝会忽略验证文件的旧二进制。R03-I26 在原有 25 个设计用例之外加入此反例；EOS 只有具备托管验证和准入路径才声明该特性。

## 集成边界与测试

当前 `Supervisor` 保存一个 `run_id`，把 `--run-id` 传给已选 EOS 子进程，把 `ISOS_RUN_ID` 传给 EGIA，并在自动子进程重启时保留。`restart_processes_with_plan` 分配候选运行；替换失败时再分配恢复运行。实现时，应将授权包作为随 supervisor 保留的运行对象，通过私有启动计划传给 `spawn`，不得表示成图 `settings.env`、`env_from` 或生成文件。`generated_v2::selected_root` 可能返回 G03 回退路径，故授权预检须先要求并验证保存修订的显式 schema-2 V2 选择/manifest；只有该叶目录提供 profile 字节。预检必须在 `managers.remove(name)` 与停止子进程之前完成。当前首次使用状态公开保存/活动修订及运行 ID；继续作为可观察身份，不加入凭据字段。

固定矩阵覆盖签发、可选接收方缺失、精确角色交付、同运行重用、显式轮换、失败重启的恢复轮换、G03 回退和保留启动实参在停止前被拒绝、文件系统攻击、事件范围和脱敏错误。单元测试仅可在测试构建中注入确定性随机字节；生产使用操作系统 CSPRNG。集成 stub 只记录收到哪些保留环境变量，不记录其值。EOS 用例重放验证运行/修订及方法矩阵；IsOS 重放验证签发与进程交付。R07 后续证明浏览器 bridge 结果追踪；R05/R06 证明物理 fence 与模拟器停止顺序。

合并前需获得独立设计及实现审查；运行 `python3 tests/fixtures/control_grants/check_cases.py`、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build` 及干净 home 的 Run/Restart/Stop 边界测试。完整输出存于忽略的 `logs/test/<run-id>/`，并在此记录精确提交、命令和结果。本文档自身不构成实现证据。

设计基线证据（2026-11-02）：IsOS 基础提交 `ace8f4c60b8a74b3a998fe2ae78da4c38afa2020`，Linux `7.0.0-38-generic` x86_64，Cargo 1.98.1。从仓库根运行 `python3 tests/fixtures/control_grants/check_cases.py`、`git diff --check`、`/home/ashex/.cargo/bin/cargo fmt --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings`、`/home/ashex/.cargo/bin/cargo test`、`/home/ashex/.cargo/bin/cargo build` 均通过。既有 Rust 套件报告 179 个单元、六个 CLI 指南、一个已移除命令和九个边准入测试。完整输出位于忽略的 `logs/test/20261102-r03-control-grants-design/`。这些门槛只检查设计文件和未变的基线；新增 25 个 R03 用例仍需实现和独立审查。

IsOS 签发方实现证据（2026-11-02）：在 Linux `7.0.0-38-generic` x86_64、Cargo 1.98.1 环境中，`python3 tests/fixtures/control_grants/check_cases.py`、`/home/ashex/.cargo/bin/cargo fmt --all -- --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings`、`/home/ashex/.cargo/bin/cargo test`、`/home/ashex/.cargo/bin/cargo build` 和 `git diff --check` 均通过。测试报告 191 个单元、六个 CLI 指南、一个已移除命令和九个边准入测试。完整输出保存在忽略的 `logs/test/20261102-r03-control-grants-impl/`。Rust 测试覆盖签发、私有文件检查、精确 V2 选择、子进程环境隔离、同一运行自动重启、停止前拒绝、候选运行终止失败、独立恢复授权，以及运行范围发布失败时撤销授权。由于已选 EOS 二进制必须证明新特性，R03-I26 将矩阵扩展到 26 项。设置 `ISOS_BIN=/home/ashex/projects/isos-admission-grants/target/debug/isos ISOS_AETHER_BIN_DIR=/home/ashex/projects/Island-eos-admission-revision/target/debug` 后运行 `python3 tests/reader_features_cli.py`，七个干净 home 用例通过；因未设置 `ISOS_REAL_EGIA_ROOT`，一个真实 EGIA 用例跳过；已选旧 EOS 的反例通过。独立 IsOS 实现审查已批准提交 `5c1cb27878d5594e9eda2f07765395e2830da045`；EOS/IsOS 本地重放已通过。R03 关闭前仍需合并已审查的 PR。

本地跨仓库证据（2026-11-02）：Linux `7.0.0-38-generic` x86_64，IsOS 提交 `5c1cb27878d5594e9eda2f07765395e2830da045`，EOS 运行时代码 `607eb0a93824c9488d17d38c9785c44f87ca4934`（所选 EOS 工作树另有纯文档后继提交）。重放在临时干净的 `ISOS_HOME` 下选用 `/home/ashex/projects/isos-admission-grants/target/debug/isos` 和 `/home/ashex/projects/Island-eos-admission-revision/target/debug`。精确候选运行使用主 IsOS 仓库中的 `logs/test/20261102-r03-cross-repo/r03-cross-replay.executed.py`；完整输出是同目录的 `mock-base-grpc-rotation.log`。合并已审查的 PR 后，从主 IsOS 根目录运行 `/home/ashex/.cargo/bin/cargo build --manifest-path logs/test/20261102-r03-cross-repo/probe/Cargo.toml --offline`，再运行 `ISOS_BIN=$PWD/target/debug/isos ISOS_AETHER_BIN_DIR=/home/ashex/projects/Island-eos/target/debug python3 logs/test/20261102-r03-cross-repo/r03-cross-replay.py` 即可复现；设置 `R03_EOS_REPO` 可选择 EOS fixture 所在仓库。使用 EOS mock CXX 底盘绑定，Validate/Apply 和显式 schema-2 V2 选择通过；EOS 加 config 在保存图的精确修订下就绪。授权包目录为 `0700`，三个文件为 `0600`，验证文件包含运行 ID／修订；只有 EOS 收到验证文件路径，config 没有授权环境变量。真实 EOS gRPC 发现返回活动修订和独立投影摘要；EGIA 授权的 `base.velocity` Submit 与 Get 成功。操作员运动返回 `PermissionDenied`，过期修订返回 `FailedPrecondition`，其他角色 Get 与未知操作返回相同的 `NotFound`。显式 Restart 在保存修订不变时轮换运行和两个授权，撤销旧授权包，新的 EOS 监听器拒绝旧授权；Stop 在子进程停止后撤销新授权包。此 mock 底盘重放不证明 MuJoCo 实体 fence 或停止顺序，后者属于 R05/R06。
