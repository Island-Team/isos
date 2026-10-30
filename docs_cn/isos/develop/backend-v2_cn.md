# 开发与验证 v2 后端链路

[English](../../../docs/isos/develop/backend-v2.md)

构建 EOS Rust 可执行文件（`aether-profile`、`aether-runtime`、`aether-http-bridge`、`aether-config-server`）、IsOS 和 MuJoCo 仿真器。使用 MuJoCo 仓库的 `tools/build-backend-bundle.py` 制作真实软件包。`isos pkg link island/mujoco-sim 2.0.0 <bundle>` 注册本地开发包；`isos pkg install` 会先校验发行归档。`isos init` 写入唯一的可编辑共享图；`isos config validate` 与 `apply` 准入并保存。图修改都必须通过同一 v2 注册表和 IsOS 软件包投影。将 Aether 可执行文件安装在 `isos` 旁边，或在生成图前将 `ISOS_AETHER_BIN_DIR` 设为绝对构建目录。`ISOS_AETHER_PROFILE_BIN` 仅覆盖验证器；`MUJOCO_ROOT` 指向 MuJoCo 库根目录。

IsOS 源模板为 `templates/mujoco-graph-v2.toml`。`src/v2_starter.rs` 选择本机路径，`src/v2_backend.rs` 解析软件包回执、描述文件与资源，`src/v2_process.rs` 校验后端/设备/套接字引用并管理直接子进程，`src/http.rs` 提供工作台端点。EOS 注册表与适配器位于自身 `crates/` 工作区；IsosUI 拥有编辑器。软件包描述文件、图节点设置和仿真器数据结构应各自独立。

可重复检查：运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python3 tests/clean_home_v2.py`。测试程序在全新 home 中制作协议模拟包，检查缺失资源、错误设备准入、删除保护、进程顺序、修订不变性、发现、相机采集、底盘与机械臂观测及运动、安全停止、重启和日志；测量五次启动/重启和 200 次 HTTP→EOS 套接字请求。此程序不测试 MuJoCo 物理或真实库加载。

2026-10-30 Linux x86_64：34 项 IsOS 测试、Clippy 和格式检查通过，clean-home 模拟场景通过。固定负载的启动延迟中位数/p99 为 346.917/350.345 毫秒；HTTP→EOS 请求延迟中位数/p99 为 10.148/10.469 毫秒，吞吐量 98.4 请求/秒。结果包含 IsOS 管理器 50 毫秒轮询周期及本地 Python 模拟。完整输出在 `logs/test/20261030-isos-package-run/`。

前台生命周期检查（2026-10-30，Linux x86_64）：`cargo build`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python3 tests/clean_home_v2.py` 均通过。测试程序让 `isos init` 与 `isos run` 保持运行，对 `run` 发送 SIGINT 并确认部署子进程全部停止；随后通过 API 重新启动，对 `init` 发送 SIGINT 并确认子进程退出且管理套接字已移除。34 项单元测试通过。五次固定启动/重启操作的延迟中位数/p99 为 303.335/345.795 毫秒；200 次 HTTP→EOS 套接字请求的延迟为 10.179/10.587 毫秒，吞吐量为 98.1 请求/秒。环境和完整输出位于 `logs/test/20261030-foreground/`。

本地前端检查（2026-10-30，Linux x86_64）：构建 `IsosUI/dist/local`，然后执行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build` 以及 `ISOS_TEST_WEB_DIST=/absolute/path/to/IsosUI/dist/local python3 tests/clean_home_v2.py`。34 项 Rust 测试和 clean-home 场景均通过。测试程序确认本地 URL 无令牌、IsosUI 页面可访问，且 SIGINT 能停止前台服务。五次固定启动/重启操作的延迟中位数/p99 为 302.741/347.765 毫秒；200 次 HTTP→EOS 套接字请求的延迟为 10.136/10.486 毫秒，吞吐量为 98.6 请求/秒。完整输出位于 `logs/test/20261030-local-frontend/`。

真实软件包检查（2026-10-30，Linux x86_64，MuJoCo 3.16.0）：运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`，再将 `ISOS_BIN`、`ISOS_TEST_REAL_BUNDLE`、`ISOS_AETHER_BIN_DIR`、`MUJOCO_ROOT`、`ISOS_WEB_DIST` 设为绝对路径并运行 `python3 tests/real_home_v2.py`。测试程序将真实包复制到全新 home，链接包，启动前台 `init`，选择场景并将 `MUJOCO_GUI=1`，通过工作台 API 验证和应用，再启动前台 `run`。它检查进程顺序、发现、真实相机采集、底盘与机械臂观测和指令、停止、重启、日志、软件包删除保护、嵌套资源变更保护、图修订及 Ctrl+C 关停。有 `DISPLAY` 和 `xwininfo` 时还确认 GUI 窗口映射状态为 `IsViewable`。34 项 Rust 测试和真实场景通过。两条运动指令按预期返回 `OutcomeUnknown`：适配器确认接收，但未报告物理动作的终态。一次启动耗时 1553.222 毫秒；20 次真实相机采集中位数/p99 为 357.572/379.177 毫秒，吞吐量 2.8 请求/秒。这仅是单机基线而非性能目标。该 Wayland/X11 主机上 MuJoCo 发出非致命 OpenGL 0x502 警告，但仍正常提供相机帧和控制请求。完整命令和输出在 `logs/test/20261030-isos-graph-audit/`。
