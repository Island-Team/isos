# 开发与验证 v2 后端链路

[English](../../../docs/isos/develop/backend-v2.md)

## 软件包节点契约更新（2026-10-30）

起始图现在直接读取 `templates/generic-graph-v2.toml`；过时的 MuJoCo 起始模板已移除。清理后 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test v2_starter`（3 项）及 clean-home 集成测试均通过。日志位于 `logs/test/20261030-package-node-contract/*generic-template.log` 与 `starter.log`。

通用接口阶段加入 `config.toml` 加载与环境变量覆盖、已验证 sidecar 和 `/node-types` 中的有类型贡献 `data_types`、软件包与内置节点混合集合的固定版本语义，以及仅所有者可访问的分类轮转日志与游标读取。Linux x86_64 上 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`、`python3 tests/clean_home_v2.py` 均通过；六节点 MuJoCo 集合包含四个固定软件包节点、内置 EOS 闸门和 IsosUI Vision。完整输出在 `logs/test/20261030-general-interfaces/`。clean-home 使用协议桩；真实 MuJoCo 测试另行记录。
运行配置还提供经过校验的 `[logging].level` 文件级别阈值，`ISOS_LOG_LEVEL` 环境变量优先。clean-home 覆盖经 EOS 输出的 MuJoCo 子进程日志及游标读取。最终真实 MuJoCo 测试通过双 Vision 闸门 WebSocket 流、源切换、生命周期和安全停止。在这台 Linux x86_64、MuJoCo 3.16.0 主机上，一次启动耗时 1527.699 ms；20 次相机采集的中位数/p99 为 385.012/448.201 ms，吞吐量为 2.6 次/秒。这些是初始基线，并非性能目标。完整输出在 `logs/test/20261030-general-interfaces/real-home-vision-final.log`。

新的 `isos init` 草稿包含 EOS 调度器和 IsOS 管理的 EOS/bridge/config 进程，不预先选择硬件包。用 `isos pkg link island/mujoco-sim 2.0.0 <bundle> --manifest <bundle>/release.json` 链接 v4 MuJoCo 包，然后以 `isos pkg preview-set island/mujoco-sim 2.0.0 sim-hardware <graph> --values <values.json>` 预览节点集合。值文件须包含描述文件中的场景 ID，以及位于 IsOS 运行目录中的套接字路径。返回的 `graph` 通过常规图操作应用。现有已应用图不会自动变更；`--replace-legacy-mujoco` 执行显式预览迁移并保留无关节点。

运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`，再运行 `ISOS_BIN=target/debug/isos AETHER_BIN_DIR=../Island-eos/target/debug python3 tests/clean_home_v2.py`。Linux x86_64 上 40 项 Rust 测试与 clean-home 协议桩均通过。协议桩检查软件包链接、合成、准入失败、进程归属、采集、观测、运动、停止、重启、修订检查和软件包删除保护。固定负载的五次启动/重启中位数/p99 为 252.165/300.935 毫秒；200 次 HTTP 至 EOS 时间请求中位数/p99 为 10.160/10.612 毫秒，吞吐量为每秒 98.4 次。使用 MuJoCo 3.16.0 暂存包，运行 `ISOS_BIN=target/debug/isos ISOS_TEST_REAL_BUNDLE=<bundle> ISOS_AETHER_BIN_DIR=../Island-eos/target/debug MUJOCO_ROOT=<mujoco-root> ISOSUI_REPO_PATH=../IsosUI python3 tests/real_home_v2.py` 也通过：GUI 可见、采集、底盘/机械臂命令与停止、重启、链接资源变更拒绝和 Ctrl+C。真实启动为 1486.053 毫秒；20 次采集中位数/p99 为 356.168/375.170 毫秒，吞吐量每秒 2.8 次。这些是单机基线。完整输出保存在 `logs/test/20261030-package-node-contract/`。以下历史运行使用较早的自动 MuJoCo 起始图。

构建 EOS Rust 可执行文件（`aether-profile`、`aether-runtime`、`aether-http-bridge`、`aether-config-server`）、IsOS 和 MuJoCo 仿真器。使用 MuJoCo 仓库的 `tools/build-backend-bundle.py` 制作真实软件包。`isos pkg link island/mujoco-sim 2.0.0 <bundle>` 注册本地开发包；`isos pkg install` 会先校验发行归档。`isos init` 写入唯一的可编辑共享图；`isos config validate` 与 `apply` 准入并保存。图修改都必须通过同一 v2 注册表和 IsOS 软件包投影。将 Aether 可执行文件安装在 `isos` 旁边，或在生成图前将 `ISOS_AETHER_BIN_DIR` 设为绝对构建目录。`ISOS_AETHER_PROFILE_BIN` 仅覆盖验证器；`MUJOCO_ROOT` 指向 MuJoCo 库根目录。

新建节点图会固定已安装的第一个 v2 MuJoCo 软件包版本及其声明的第一个场景。2026-10-30 使用 `/home/ashex/.cargo/bin/isos` 和同目录 Aether release 可执行文件进行了无二进制路径覆盖的检查：`pkg link`、前台 `init`、`v2 validate`、`v2 apply` 和前台 `run` 均通过。四个子进程依赖顺序启动，Ctrl+C 依赖顺序停止；所选场景为 `pick-place`。命令与节点图记录位于 `logs/test/20261030-starter-scene/installed-home.log`。

IsOS 源模板为 `templates/generic-graph-v2.toml`。`src/v2_starter.rs` 选择本机路径，`src/v2_backend.rs` 解析软件包回执、描述文件与资源，`src/v2_process.rs` 校验后端/设备/套接字引用并管理直接子进程，`src/http.rs` 仅提供环回 API。`src/main.rs` 从 `ISOSUI_REPO_PATH` 或相邻的 `../IsosUI` 运行并管理 IsosUI 的 `npm start` 进程组，设置 `ISOSUI_PORT` 和 `ISOSUI_API_ORIGIN`。按 Ctrl+C 会停止 npm 子进程、API、管理器和部署子进程。EOS 注册表与适配器位于自身 `crates/` 工作区；IsosUI 拥有编辑器。软件包描述文件、图节点设置和仿真器数据结构应各自独立。

可重复检查：运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python3 tests/clean_home_v2.py`。测试程序在全新 home 中制作协议模拟包，检查缺失资源、错误设备准入、删除保护、进程顺序、修订不变性、发现、相机采集、底盘与机械臂观测及运动、安全停止、重启和日志；测量五次启动/重启和 200 次 HTTP→EOS 套接字请求。此程序不测试 MuJoCo 物理或真实库加载。

2026-10-30 Linux x86_64：34 项 IsOS 测试、Clippy 和格式检查通过，clean-home 模拟场景通过。固定负载的启动延迟中位数/p99 为 346.917/350.345 毫秒；HTTP→EOS 请求延迟中位数/p99 为 10.148/10.469 毫秒，吞吐量 98.4 请求/秒。结果包含 IsOS 管理器 50 毫秒轮询周期及本地 Python 模拟。完整输出在 `logs/test/20261030-isos-package-run/`。

前台生命周期检查（2026-10-30，Linux x86_64）：`cargo build`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python3 tests/clean_home_v2.py` 均通过。测试程序让 `isos init` 与 `isos run` 保持运行，对 `run` 发送 SIGINT 并确认部署子进程全部停止；随后通过 API 重新启动，对 `init` 发送 SIGINT 并确认子进程退出且管理套接字已移除。34 项单元测试通过。五次固定启动/重启操作的延迟中位数/p99 为 303.335/345.795 毫秒；200 次 HTTP→EOS 套接字请求的延迟为 10.179/10.587 毫秒，吞吐量为 98.1 请求/秒。环境和完整输出位于 `logs/test/20261030-foreground/`。

本地前端检查（2026-10-30，Linux x86_64）：构建 `IsosUI/dist/local`，然后执行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build` 以及 `ISOS_TEST_WEB_DIST=/absolute/path/to/IsosUI/dist/local python3 tests/clean_home_v2.py`。34 项 Rust 测试和 clean-home 场景均通过。测试程序确认本地 URL 无令牌、IsosUI 页面可访问，且 SIGINT 能停止前台服务。五次固定启动/重启操作的延迟中位数/p99 为 302.741/347.765 毫秒；200 次 HTTP→EOS 套接字请求的延迟为 10.136/10.486 毫秒，吞吐量为 98.6 请求/秒。完整输出位于 `logs/test/20261030-local-frontend/`。

真实软件包检查（2026-10-30，Linux x86_64，MuJoCo 3.16.0）：运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`，再将 `ISOS_BIN`、`ISOS_TEST_REAL_BUNDLE`、`ISOS_AETHER_BIN_DIR`、`MUJOCO_ROOT`、`ISOS_WEB_DIST` 设为绝对路径并运行 `python3 tests/real_home_v2.py`。测试程序将真实包复制到全新 home，链接包，启动前台 `init`，选择场景并将 `MUJOCO_GUI=1`，通过工作台 API 验证和应用，再启动前台 `run`。它检查进程顺序、发现、真实相机采集、底盘与机械臂观测和指令、停止、重启、日志、软件包删除保护、嵌套资源变更保护、图修订及 Ctrl+C 关停。有 `DISPLAY` 和 `xwininfo` 时还确认 GUI 窗口映射状态为 `IsViewable`。34 项 Rust 测试和真实场景通过。两条运动指令按预期返回 `OutcomeUnknown`：适配器确认接收，但未报告物理动作的终态。一次启动耗时 1553.222 毫秒；20 次真实相机采集中位数/p99 为 357.572/379.177 毫秒，吞吐量 2.8 请求/秒。这仅是单机基线而非性能目标。该 Wayland/X11 主机上 MuJoCo 发出非致命 OpenGL 0x502 警告，但仍正常提供相机帧和控制请求。完整命令和输出在 `logs/test/20261030-isos-graph-audit/`。

npm 前端分离检查（2026-10-30，Linux x86_64）：运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`、`python3 tests/clean_home_v2.py`；真实 MuJoCo 场景需设置 `ISOS_BIN`、`ISOS_TEST_REAL_BUNDLE`、`ISOS_AETHER_BIN_DIR`、`MUJOCO_ROOT` 和 `ISOSUI_REPO_PATH` 后运行 `python3 tests/real_home_v2.py`。clean-home 测试使用最小 npm 前端桩，真实测试使用 IsosUI 的 `npm start`。两个场景及 37 项 Rust 测试均通过。直接检查中，`127.0.0.1:16091/en/isos` 返回 200，`127.0.0.1:16090/api/isos/v2/version` 返回 200，后端前端路径返回 404；按 Ctrl+C 后两个端口都关闭。五次固定模拟启动/重启的延迟中位数/p99 为 304.898/353.374 毫秒；200 次 HTTP→EOS 请求的延迟为 10.168/10.887 毫秒，吞吐量 98.3 请求/秒。真实 MuJoCo 启动为 1635.447 毫秒；20 次相机采集的中位数/p99 为 358.120/480.780 毫秒，吞吐量 2.7 次/秒，GUI 窗口可见。这些仅为单机基线。完整输出在 `logs/test/20261030-npm-split/`。

## EOS 管理 MuJoCo 的边界（2026-10-30）

当前图将 `eos.process.backend@v1.0` 放在 `cores/eos/layers/process/groups/backend/nodes/simulator`。IsOS 仍在校验、应用和运行时检查固定版本的软件包、场景、资源、库、套接字和设备 ID。启动时，IsOS 向 `aether-runtime` 传入 `--backend-node`、`--backend-executable`、`--backend-scene`，并将已解析的库环境传给 EOS 子进程。EOS 管理仿真器子进程并等待套接字就绪。IsOS 只直接管理 `eos`、`bridge` 和 `config`。HTTP 桥与只读图目录默认监听环回端口 16020 和 16021。`isos init` 保留现有编写图；升级 IsOS 和 EOS 后，应备份旧图，再明确创建并应用新图。

Linux x86_64 验证：`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`（37 项）、`cargo build`、设置 `ISOS_BIN` 和 `AETHER_BIN_DIR` 后运行 `python3 tests/clean_home_v2.py`，以及设置 `ISOS_BIN`、`ISOS_TEST_REAL_BUNDLE`、`ISOS_AETHER_BIN_DIR`、`MUJOCO_ROOT`、`ISOSUI_REPO_PATH` 后运行 `python3 tests/real_home_v2.py`，全部通过。模拟仿真器确认其父 PID 是 EOS 的 PID，且 IsOS 子进程列表中没有仿真器。真实 MuJoCo GUI、相机、运动、停止、重启与链接资源检查通过。五次固定模拟启动/重启的中位数/p99 为 249.150/290.961 毫秒；200 次 HTTP→EOS 时间请求为 10.184/10.698 毫秒、98.3 请求/秒。真实启动耗时 1483.387 毫秒；20 次相机采集为 354.687/445.432 毫秒、2.8 次/秒。这些只是单机基线。完整输出位于 `logs/test/20261030-eos-backend-delegation/`。

## EOS 控制起始图与真实默认部署（2026-10-30）

起始图增加一个手动底盘控制、底盘速度与机械臂关节原语、以及两步演示技能。每个节点都有明确的 `settings` 与 `references`；有类型的 `status` 和硬件观测输入使 IsosUI 中的连线可见。现有调度器→执行器命令边仍是唯一的执行器路径。旧图不会自动增加这些节点，必须明确应用新图。

用户原图已复制到 `~/.local/share/isos/archive/2026-10-30-eos-owned-backend/default.nodes.json.before-eos-owner`，SHA-256 同为 `74534f22d7ff0661f2568fd9f519634da8c74a698b0454311415ef9e7052b66f`。原图使用场景 `default`（已链接的 2.0.0 包只有 `pick-place` 和 `navigation`）、IsOS 所有的后端节点以及不存在的 `/usr/bin` Aether 命令。明确修正的候选图通过校验并以修订 `0c998e2760e536a2f44313b869c3c3b8f373574c69d74325be0639bef87e156d` 应用。使用已安装的 IsOS/EOS master 可执行文件，真实默认部署在 16000/16001 启动 IsOS API/UI，在 16020/16021 启动 EOS HTTP 桥/图目录，MuJoCo 是 EOS 的直接子进程。GUI 可见；发现、相机采集、底盘观测、`isos stop` 与 Ctrl+C 均通过。EGIA 尚无执行进程，因此 16060 未监听。完整步骤和已安装文件的哈希位于 `logs/test/20261030-live-eos-owned/`。

控制起始图分支执行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`，设置 `ISOS_BIN` 与 `AETHER_BIN_DIR` 后运行 `python3 tests/clean_home_v2.py`，并按上文真实包命令运行 `python3 tests/real_home_v2.py`。clean-home 场景检查四个控制节点、手动/原语请求、遇到未知物理结果时安全停止的异步技能、可重复的技能取消、使用自定义 IsosUI 端口的浏览器 Origin 预检和 POST，以及仿真器父 PID。五次固定模拟启动/重启的中位数/p99 为 253.713/300.272 毫秒；200 次 HTTP→EOS 时间请求为 10.143/11.420 毫秒、98.3 请求/秒。含手动/原语/技能控制的真实 GUI 运行通过；启动耗时 1506.944 毫秒，20 次相机采集为 349.897/365.336 毫秒、2.9 次/秒。这些只是单机基线。完整输出位于 `logs/test/20261030-starter-control/`。

## Rust 程序集合与通用设备目录（2026-10-31）

使用 `isos build eos` 或向 `/api/isos/v2/build-binaries` POST `{ "schema_version":2, "target":"eos", "install_deps":false }` 构建 EOS。两者共用 Rust 工作区构建及私有、摘要校验的暂存服务。Run 之前须通过 `PUT /node-graph` 应用返回的 `suggested_graph`。管理器对精确的 EOS runtime、bridge、config server、profile 程序检查 `--capabilities`，包括 `short-paths`、`node-types`、`data-idl`、`primitive-binding`、runtime 的 `log-dir` 和 bridge 的 `vision-ws`。缺少能力时启动直接报错，不会把降级状态当成成功。为保证构建收据可重现，未提交修改的 EOS 工作区会被拒绝。

本地检查：`/home/ashex/.cargo/bin/cargo fmt --check`、`/home/ashex/.cargo/bin/cargo clippy --all-targets -- -D warnings`、`/home/ashex/.cargo/bin/cargo test`（52 项通过）及 `/home/ashex/.cargo/bin/cargo build` 均通过。通过 `isos pkg link island/mujoco-sim 2.0.0 BUNDLE --manifest BUNDLE/release.json` 链接通用包，并以运行目录内的 `socket` 值预览 `sim-hardware`。在隔离的 `ISOS_HOME` 中，预览加入八个标准节点，通过 EOS 注册表及 IsOS 软件包投影校验。对该预览的实时 `POST /node-options` 返回已编译且可用的 `mujoco-v2`、两个原语绑定，以及可用的相机、底盘、机械臂目标。命令和完整检查输出在 `logs/test/20261031-rust-build-catalog/`。当前分支尚未实现 IsOS 浏览器网关；Vision WebSocket 请求仍使用 EOS bridge 地址。

EOS 提交 `3826640f` 后工作区已干净，运行 `ISOS_TEST_BUILD_STAGE=1 ISOS_BIN=/home/ashex/projects/isos/target/debug/isos AETHER_BIN_DIR=/home/ashex/projects/Island-eos/target/debug python3 tests/clean_home_v2.py` 并通过。CLI Build 将五个程序暂存并写入摘要收据；API Build 返回一致的路径与摘要及未应用的建议图；起始图与 Run 使用这些暂存程序；模拟相机／指令、停止、旧程序 HTTP 422 检查均通过。完整输出在 `logs/test/20261031-rust-build-catalog/build-stage-clean-home.log`。这是功能集成检查，不是性能目标；本机五次启动／重启中位数为 1760.809 毫秒，p99 为 1867.731 毫秒。

日志游标身份现在同时包含文件创建时间、设备号和 inode，因此文件系统在快速轮转时复用 inode 也不会把新文件误认成旧文件。轮转专项测试连续运行 100 次通过；输出在 `logs/test/20261031-rust-build-catalog/cursor-repeat.log`。

默认部署现场修复（2026-10-31）：`isos build eos` 暂存 EOS master `3826640f`；Build API 建议三个进程的可执行文件路径，带修订校验的 Apply 保留原图。旧版链接的 MuJoCo 2.0.0 包缺少 `node_contributions`，因此换成从 MuJoCo master `43c9ea5` 构建的包。对原图副本运行 `isos pkg preview-set ... --replace-legacy-mujoco` 时，最初留下空的旧 `process/backend` 组，EOS 拒绝该图。现在迁移会清理空组，并把旧模拟器环境设置带入新进程节点。最终八节点预览通过验证，保留手动、primitive 和技能控制，应用修订号为 `9d8e7498d286c0b29eb1eec5b6cd659ea5957e8eae7d8dcdba376eab4bd0d2b8`。Run 启动 EOS、bridge、config；门节点发现返回 30 Hz 的 `eos/interface/gate/front-preview`，浏览器 WebSocket 收到 640×480 JPEG。原图与暂存图备份在 `~/.local/share/isos/archive/2026-10-31-live-repair/`。单元检查：`cargo test node_sets --bin isos` 三个测试通过，并完成现场预览和运行检查。
