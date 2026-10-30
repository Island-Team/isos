# 开发与验证 v2 后端链路

[English](../../../docs/isos/develop/backend-v2.md)

构建 EOS Rust 可执行文件（`aether-profile`、`aether-runtime`、`aether-http-bridge`、`aether-config-server`）、IsOS 和 MuJoCo 仿真器。使用 MuJoCo 仓库的 `tools/build-backend-bundle.py` 制作真实软件包。`isos pkg link island/mujoco-sim 2.0.0 <bundle>` 注册本地开发包；`isos pkg install` 会先校验发行归档。`isos init` 写入唯一的可编辑共享图；`isos config validate` 与 `apply` 准入并保存。图修改都必须通过同一 v2 注册表和 IsOS 软件包投影。验证器不在 `PATH` 时设置 `ISOS_AETHER_PROFILE_BIN`；`MUJOCO_ROOT` 指向 MuJoCo 库根目录。

IsOS 源模板为 `templates/mujoco-graph-v2.toml`。`src/v2_starter.rs` 选择本机路径，`src/v2_backend.rs` 解析软件包回执、描述文件与资源，`src/v2_process.rs` 校验后端/设备/套接字引用并管理直接子进程，`src/http.rs` 提供工作台端点。EOS 注册表与适配器位于自身 `crates/` 工作区；IsosUI 拥有编辑器。软件包描述文件、图节点设置和仿真器数据结构应各自独立。

可重复检查：运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python3 tests/clean_home_v2.py`。测试程序在全新 home 中制作协议模拟包，检查缺失资源、错误设备准入、删除保护、进程顺序、修订不变性、发现、相机采集、底盘与机械臂观测及运动、安全停止、重启和日志；测量五次启动/重启和 200 次 HTTP→EOS 套接字请求。此程序不测试 MuJoCo 物理或真实库加载。

2026-10-30 Linux x86_64：34 项 IsOS 测试、Clippy 和格式检查通过，clean-home 模拟场景通过。固定负载的启动延迟中位数/p99 为 346.917/350.345 毫秒；HTTP→EOS 请求延迟中位数/p99 为 10.148/10.469 毫秒，吞吐量 98.4 请求/秒。结果包含 IsOS 管理器 50 毫秒轮询周期及本地 Python 模拟。完整输出在 `logs/test/20261030-isos-package-run/`。真实仿真器仍需在具有 `MUJOCO_ROOT` 和已构建 C++ 可执行文件的主机上测量。

前台生命周期检查（2026-10-30，Linux x86_64）：`cargo build`、`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python3 tests/clean_home_v2.py` 均通过。测试程序让 `isos init` 与 `isos run` 保持运行，对 `run` 发送 SIGINT 并确认部署子进程全部停止；随后通过 API 重新启动，对 `init` 发送 SIGINT 并确认子进程退出且管理套接字已移除。34 项单元测试通过。五次固定启动/重启操作的延迟中位数/p99 为 303.335/345.795 毫秒；200 次 HTTP→EOS 套接字请求的延迟为 10.179/10.587 毫秒，吞吐量为 98.1 请求/秒。环境和完整输出位于 `logs/test/20261030-foreground/`。
