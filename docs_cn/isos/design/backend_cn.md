# IsOS 管理的后端软件包

[English](../../../docs/isos/design/backend.md)

MuJoCo 是首个 Aether 后端包：`island/mujoco-sim@0.1.1`。包内含 `backend-v2.json`、`bin/eos-mujoco-sim`、场景和资源；描述文件名是当前包的兼容身份。发行清单描述分发与归档校验；后端描述文件声明可执行文件、场景 ID、设备 ID、协议和运行依赖。当前 `1.0.0` 图使用包贡献的 `eos.process.mujoco@v1.0` 节点；`0.1.0` 写入器使用 `@v0.1`。三者职责不同，安装软件包不会暗中修改部署。

IsOS 在校验、应用和运行时解析精确固定版本。每次解析都会重新读取本地链接；检查描述文件身份、执行权限、所选场景、场景引用资源、MuJoCo 库、规范套接字路径，以及 EOS 适配器的设备和套接字引用。描述文件、可执行文件、场景和资源的指纹能发现运行中链接的变化。用户在图中选择场景并明确应用。IsOS 启动 EOS 时传入已验证的可执行文件与场景绝对路径。EOS 读取图、启动 MuJoCo 子进程，并用有类型的相机、底盘和机械臂适配器连接套接字；EOS 不能安装软件包或修改已应用图。

套接字位于仅所有者可访问的 IsOS 运行目录下。IsOS 先启动 EOS；EOS 启动 MuJoCo 并等待套接字就绪，然后才允许硬件操作。IsOS 管理 EOS、HTTP 桥和配置服务的子进程，EOS 管理仿真器子进程。停止 EOS 会终止 MuJoCo；仿真器意外退出时 EOS 失败，由 IsOS 重启整个 EOS 组件。任何已应用图仍引用精确软件包 ID 和版本时禁止删除。后续后端可复用同一进程边界；执行器类型仍需通过 EOS 安全准入。

**0.x 目标：**IsOS 从已验证包元数据通过[设置选项 API](../interfaces/local-api_cn.md#发行目标设置选项尚未实现)解析稳定场景与设备 ID。EOS 内部适配器注册表保存已编译的 MuJoCo 套接字适配器；外部仿真器包仍由 IsOS 安装。即使包声明了适配器，EOS 若未编译相应实现也必须拒绝。IsOS 记录精确包版本及已验证生成配置，不把仿真器复制进 EOS，也不允许包代码添加第二条执行器路由。参见[准入、运行与停止算法](system_cn.md)。

**P02 实现。**只使用 P01 的精确已验证包句柄。按 `catalog_version` 分类 `backend-v2.json`：完整旧结构只供历史图使用，完整 `0.1.0` 结构发布场景／设备元数据，部分或未知结构关闭准入。对每个所选场景，用安全 XML 解析结果比较 MJCF 标记及所需模型元素，比较 SHA-256 与已验证 XML 字节，再检查通用 EOS 适配器所选设备 ID、类别和 `scene_ids` 成员关系。P01 负载摘要覆盖引用资源；已封存负载变化先在该层失败。在 Apply 和启动前检查 `MUJOCO_ROOT` 库可用性，并在就绪时确认实际加载版本。[IsOS 契约](../interfaces/mujoco-catalog_cn.md)与[归属方夹具](../../../tests/fixtures/scene_catalog/README.md)定义精确字段、路径与顺序。

描述文件解析器接受目标增量字段；节点集合合成器要求新预设使用 `0.1.0` 图，同时保留历史 `1.0.0` 集合。历史解析路径及安全旧图行为保持独立：即使当前侧车含新预设，历史描述文件也不能发布它。只合成草稿，使用包进程类型及 EOS 通用相机、底盘、机械臂、原语绑定、闸门类型；每个通用硬件适配器要求 `simulator_node == device_id ==` 该类别所选设备 ID，而 backend 引用指向进程节点。不重复起始 Dispatcher，也不增加 Vision 身份。P01 跨仓库 CLI 夹具使用归属方提供的 `@v0.1` 进程身份，不重复追加。P03 另行负责通用 node-options 展示。

`isos init` 仅首次写入起始图。Home 展示已安装或链接的包、场景与先决条件问题、保存及运行中修订、运行状态和日志。Graph 编辑唯一的 IsOS/EOS/EGIA 共享图。EGIA common 节点目前仅用于设计。可视布局与运行配置分开保存。

## 开发检查

使用新的 `ISOS_HOME` 执行 `isos init --mode dev`，在 EOS 和 IsosUI 各自仓库构建并配置其路径，然后启动 `isos serve`。用 `isos pkg link island/mujoco-sim 0.1.1 <bundle> --manifest <bundle>/release.json` 链接暂存包，预览其 `sim-hardware` 节点集合，再执行 `isos config validate <graph>`、`isos config apply <graph>` 和 `isos run`。`isos deployment stop` 只停止图，`isos stop` 关闭管理器及所有所属子进程。`MUJOCO_ROOT` 须指向已安装的 MuJoCo 运行环境，EOS 的 `aether-profile` 验证器和 Rust 可执行文件须可用。参见[节点字段](../interfaces/node-graph_cn.md)和[软件包命令](../usage/packages_cn.md)。

图运行恢复改动通过了 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`（69 项测试）。`cargo build && python3 tests/clean_home.py` 通过 IsOS→EOS→模拟 MuJoCo 生命周期场景。本机固定工作负载的套接字基线为每秒 98.9 次请求，中位延迟 10.102 毫秒，p99 延迟 11.210 毫秒。完整输出保存在本地忽略的 `logs/test/20261031-graph-run-recovery/` 目录中。

P02 在 IsOS 仓库运行 `cargo fmt --all -- --check`、`cargo clippy --all-targets --no-default-features -- -D warnings`、`cargo test --no-default-features`、`cargo build --no-default-features`、`python3 tests/package_verification_cli.py` 和 `python3 tests/scene_device_catalog_cli.py`。最后一项构建真实归属方包，在隔离主目录中链接，预览 `0.1.0` 集合，校验并应用，启动真实仿真器，然后检查错误场景／设备、缺失库、不完整目录及已封存负载篡改。须预先构建 EOS Rust 程序、MuJoCo 仿真器，并安装 MuJoCo 3.16.0 和本地 IsosUI；具体路径见测试文件开头。2026-11-01 的 Cargo 检查通过（106 项单元测试和 15 项集成测试），P01 全新主目录重放通过 7 项，P02 真实包重放在 102.981 秒内通过 4 项。完整输出位于本地忽略的 `logs/test/20261101-p02-scene-device-catalog/`。此运行只验证正确性，不测吞吐量或延迟。
