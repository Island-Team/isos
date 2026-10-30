# 开发与验证 v2 管理器

[English](../../../docs/isos/develop/backend-v2.md)

当前链路由 Rust IsOS、选中的 Rust Aether 可执行文件和 EOS 持有的软件包化 MuJoCo 仿真器组成。通用起始图来自 `templates/generic-graph-v2.toml`，不包含硬件包。`src/v2_starter.rs` 填写本机路径；`src/v2_build.rs` 暂存并选择精确的 EOS 收据；`src/v2_backend.rs` 校验软件包；`src/v2_process.rs` 管理图子进程；`src/services.rs` 在图外管理显式启动的服务。`src/http.rs` 在不同端口提供环回 API 和选中的静态界面。仅 `isos serve --ui-live` 运行 npm。

## 可重复检查

从干净的源码目录运行仓库检查，并将完整输出保存在被忽略的 `logs/test/<run-id>/`：

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
ISOS_BIN=target/debug/isos AETHER_BIN_DIR=../Island-eos/target/debug python3 tests/clean_home_v2.py
ISOS_BIN=target/debug/isos python3 tests/service_manager_v2.py
```

clean-home 测试使用隔离的 `ISOS_HOME` 和端口、MuJoCo 协议桩及显式的 EOS 构建/选择。它检查两次只初始化、API 和界面生命周期、图校验与应用、启动和重启、失败准入、日志及 Ctrl+C 清理。服务测试使用桩检查显式启动、状态、幂等性、单独停止及整体停止。这些测试不能证明真实 MuJoCo 物理或 Island-models 运行时行为。

真实仿真器检查需要将路径设为已有产物：

```sh
ISOS_BIN=target/debug/isos \
ISOS_TEST_REAL_BUNDLE=/absolute/path/to/bundle \
ISOS_AETHER_BIN_DIR=../Island-eos/target/debug \
MUJOCO_ROOT=/absolute/path/to/mujoco \
ISOSUI_REPO_PATH=../IsosUI \
python3 tests/real_home_v2.py
```

真实测试需要兼容的 MuJoCo 包和运行环境；只有这些依赖可用时才运行。产品模式检查需选择干净的 IsOS、EOS 和静态 UI 收据，并在无需 npm 或源码目录的隔离环境中提供服务。解释运行结果前先执行 `isos version check`。参见[入门](../usage/get-started_cn.md)、[本地 API](../interfaces/local-api_cn.md)和[服务契约](../interfaces/services_cn.md)。

## 历史测量

`logs/test/20261030-*/` 中 2026-10-30 的命令记录和测量使用较早的起始图，部分场景还使用前台 `isos init` 或 npm 界面。它们是历史基线，不是当前 CLI 的操作说明。2026-10-31 的管理器改造检查在 `logs/test/20261031-manager/` 和 `logs/test/20261031-manager-services/`；当时工作区缺少真实 MuJoCo 包和 `MUJOCO_ROOT`，真实测试仅通过语法检查，未运行。
