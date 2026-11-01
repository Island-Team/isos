# R03 启动授权交接设计

[English](../../../docs/isos/design/control-grants.md) · [所有者契约](../interfaces/control-grants_cn.md) · [用例](../../../tests/fixtures/control_grants/README_cn.md)

**目标设计，尚未实现。** R03 使 IsOS 成为 EOS 托管 Dispatcher 权限的签发方。签发属于管理器的 Run/Restart 事务，不属于 Apply 或不可变的 P04 生成代。已保存图确定精确修订和接收方，但不含凭据字段。所选 P04 叶目录提供已验证的 EOS 投影；独立的私有运行时授权包绑定运行及其修订。

## 事务

1. 在部署锁下重读保存字节，验证所选 schema-2 叶目录、精确软件包 pin、所有者渲染、已选二进制、读取器特性及所有 `env_from` 值。旧图、接收方缺失或代变化必须在停止活动子进程前被拒绝。候选 `active_revision` 使用最终保存字节的 SHA-256。
2. 仅当 Start 创建运行或 Restart 替换运行时分配新的部署 `run_id`。通过操作系统 CSPRNG 独立生成两个授权。使用 no-follow/exclusive 创建全新私有运行目录及三个文件，严格权限和大小，fsync 文件及目录。验证文件绑定运行 ID 与修订。不得将授权包加入 `manifest.json` 或代摘要；相同修订的 Restart 也应轮换授权，且不改变 Apply 输出。
3. 授权包持久化后，IsOS 才可停止旧进程。按依赖顺序启动候选子进程，把验证文件路径传给 EOS、EGIA 授权路径传给 EGIA、操作员授权路径传给 bridge。先从继承环境和图作者设置的子进程环境中移除三个保留变量。未选进程不获授权。现有 `ISOS_RUN_ID`/`--run-id` 与有效 `--profile` 输入必须匹配验证文件；子进程不能自行选择另一个图或运行。
4. 等待超过 TCP 绑定的 EOS 所有者就绪：manifest 与授权已验证、发现完整、写入准入状态正确。EGIA 和 bridge 的就绪依赖其被允许的 EOS 路径。整个依赖链通过后才发布 `deployment_ready`。子进程失败时，先停止候选依赖方，再停止提供方，记录候选运行终止事件，回收进程后撤销授权包。
5. 显式 Restart 时，旧子进程停止并回收后才撤销旧授权包。替换失败后若恢复旧进程规格，必须使用**新**运行 ID 和授权包；若恢复授权签发或交付失败，部署保持 degraded。不得以旧运行凭据重启旧规格。同一活动运行中的自动子进程重启保留授权包，并在重用前验证其常规文件，便于查询丢失响应的操作。Stop 在回收全部接收方后撤销授权包。管理器进程重启不得为新运行选择孤立的旧授权包；只有确认相关子进程已退出后才能清理。

运行授权包是权限能力；祖先符号链接、错误 UID、宽松权限、特殊文件、目标冲突、截断十六进制、缺少换行、额外字段、摘要变化、运行或修订不匹配都必须在就绪前失败。文件描述符和路径读取执行 no-follow 检查；接收方边界再次检查权限。停止旧进程前准备的候选授权包不会附着到旧进程。清理错误以脱敏本地诊断报告，不能使未验证子进程变为 ready。文件内容、授权摘要、bearer 元数据和运动实参均不得进入事件、状态或图错误体。

## 集成边界与测试

当前 `Supervisor` 保存一个 `run_id`，把 `--run-id` 传给已选 EOS 子进程，把 `ISOS_RUN_ID` 传给 EGIA，并在自动子进程重启时保留。`restart_processes_with_plan` 分配候选运行；替换失败时再分配恢复运行。实现时，应将授权包作为随 supervisor 保留的运行对象，通过私有启动计划传给 `spawn`，不得表示成图 `settings.env`、`env_from` 或生成文件。现有 `generated_v2::selected_root` 验证不可变 P04 叶目录，应继续提供所选修订和 profile 字节。预检必须在 `managers.remove(name)` 与停止子进程之前完成。当前首次使用状态公开保存/活动修订及运行 ID；继续作为可观察身份，不加入凭据字段。

固定矩阵覆盖签发、精确角色交付、同运行重用、显式轮换、失败重启的恢复轮换、停止前拒绝、文件系统攻击和脱敏错误。单元测试仅可在测试构建中注入确定性随机字节；生产使用操作系统 CSPRNG。集成 stub 只记录收到哪些保留环境变量，不记录其值。EOS 用例重放验证运行/修订及方法矩阵；IsOS 重放验证签发与进程交付。R07 后续证明浏览器 bridge 结果追踪；R05/R06 证明物理 fence 与模拟器停止顺序。

合并前需获得独立设计及实现审查；运行 `python3 tests/fixtures/control_grants/check_cases.py`、`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build` 及干净 home 的 Run/Restart/Stop 边界测试。完整输出存于忽略的 `logs/test/<run-id>/`，并在此记录精确提交、命令和结果。本文档自身不构成实现证据。

设计基线证据（2026-11-02）：IsOS 基础提交 `ace8f4c60b8a74b3a998fe2ae78da4c38afa2020`，Linux `7.0.0-38-generic` x86_64，Cargo 1.98.1。从仓库根运行 `python3 tests/fixtures/control_grants/check_cases.py`、`git diff --check`、`/home/ashex/.cargo/bin/cargo fmt --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings`、`/home/ashex/.cargo/bin/cargo test`、`/home/ashex/.cargo/bin/cargo build` 均通过。既有 Rust 套件报告 179 个单元、六个 CLI 指南、一个已移除命令和九个边准入测试。完整输出位于忽略的 `logs/test/20261102-r03-control-grants-design/`。这些门槛只检查设计文件和未变的基线；新增 19 个 R03 用例仍需实现和独立审查。
