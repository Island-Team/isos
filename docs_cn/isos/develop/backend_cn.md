# 开发与验证 v2 管理器

[English](../../../docs/isos/develop/backend.md)

当前链路由 Rust IsOS、EOS 预构建的 Rust Aether 程序、EGIA 启动程序以及 EOS 持有的软件包化 MuJoCo 仿真器组成。通用起始图来自 `templates/generic-graph.toml`，不包含硬件包。`src/starter.rs` 填写本机参数；`src/core_setup.rs` 调用各核心的 setup 适配器；`src/backend.rs` 校验软件包；`src/process.rs` 管理图子进程；`src/services.rs` 在图外管理显式启动的服务。`src/http.rs` 在不同端口提供环回 API 和已配置的静态界面。仅 `isos serve --ui-live` 运行 npm。

在 EOS、EGIA 与 IsosUI 各自的仓库构建。于 `config/isos.toml` 设置 `paths.aether_bin_dir`、`paths.egia_bin_dir` 和 `paths.ui_dist`，然后运行 `isos version check`。`isos build` 和按提交选择的 `isos version select` 已退出 v2 工作流。起始图的 `isos.process.component@v1.1` 节点声明 `role` 与 `args`；IsOS 从已配置核心目录解析程序。Apply 校验已保存图修订并原子写入生成的核心文件；Run 和 Restart 再次验证输入，并在启动前探测每个已启用核心。

## 可重复检查

从干净的源码目录运行仓库检查，并将完整输出保存在被忽略的 `logs/test/<run-id>/`：

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
ISOS_BIN=target/debug/isos AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" python3 tests/clean_home.py
ISOS_BIN=target/debug/isos python3 tests/service_manager.py
```

clean-home 测试使用隔离的 `ISOS_HOME` 和端口、MuJoCo 协议桩及已配置的 EOS 程序。它检查两次只初始化、API 和界面生命周期、图校验与应用、启动和重启、失败准入、日志及 Ctrl+C 清理。服务测试使用桩检查显式启动、状态、幂等性、单独停止及整体停止。这些测试不能证明真实 MuJoCo 物理或 Island-models 运行时行为。

真实仿真器检查需要将路径设为已有产物：

```sh
ISOS_BIN=target/debug/isos \
ISOS_TEST_REAL_BUNDLE=/absolute/path/to/bundle \
ISOS_AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" \
MUJOCO_ROOT=/absolute/path/to/mujoco \
ISOSUI_DIST_DIR="$(realpath ../IsosUI/dist/local)" \
python3 tests/real_home.py
```

真实测试需要兼容的 MuJoCo 包和运行环境；只有这些依赖可用时才运行。产品模式检查需配置经过验证的 EOS、EGIA 和静态 UI 产物，并在无需 npm 或源码构建的隔离环境中提供服务。解释运行结果前先执行 `isos version check`。参见[入门](../usage/get-started_cn.md)、[本地 API](../interfaces/local-api_cn.md)和[服务契约](../interfaces/services_cn.md)。

## 历史测量

`logs/test/20261030-*/` 中 2026-10-30 的命令记录和测量使用较早的起始图，部分场景还使用前台 `isos init` 或 npm 界面。它们是历史基线，不是当前 CLI 的操作说明。2026-10-31 的管理器改造检查在 `logs/test/20261031-manager/` 和 `logs/test/20261031-manager-services/`；当时工作区缺少真实 MuJoCo 包和 `MUJOCO_ROOT`，真实测试仅通过语法检查，未运行。

## 已配置构建验证（2026-10-31）

使用 IsOS 2.0.0、`../Island-eos/target/debug` 中的 EOS 工作区程序、IsosUI 的 `dist/local` 以及本机 MuJoCo 3.16.0，以下检查通过。完整输出保存在忽略 Git 的 `logs/test/20261031-configured-core-final/`。

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
ISOS_BIN=target/debug/isos AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" ISOSUI_DIST_DIR="$(realpath ../IsosUI/dist/local)" python3 tests/clean_home.py
ISOS_BIN=target/debug/isos ISOS_AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" ISOSUI_DIST_DIR="$(realpath ../IsosUI/dist/local)" ISOS_TEST_REAL_BUNDLE=../isos-mujoco-sim/logs/test/20261031-generic-device-preset/bundle MUJOCO_ROOT=/home/ashex/mujoco-3.16.0-linux-x86_64/mujoco-3.16.0 python3 tests/real_home.py
```

Rust 测试通过 77 个单元测试和五个 CLI 测试。桩仿真路线覆盖应用、启动、重启、失败准入和停止。真实仿真路线覆盖相机帧、底盘和机械臂命令、嵌套资源变更拒绝、重启和 Ctrl+C 停止。在这台主机上，真实运行启动耗时 7.04 秒；20 次捕获的中位延迟为 376.25 毫秒、p99 为 428.04 毫秒，吞吐为每秒 2.6 次请求。这些只是该主机和场景的初始测量，不是性能目标。另一次检出构建测试确认：应用图后修改 `aether-runtime` 会将就绪状态变为阻止，并防止运行时启动硬件。

## 已保存 setup 状态恢复（2026-10-31）

当前 EOS profile 接受适配器类型后，使用 `isos config apply` 重新应用了已保存的 MuJoCo 图。成功 Apply 现在会清除之前的 setup 错误；就绪报告会将保存图中不存在的核心标记为未启用。`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test` 均通过（78 个单元测试、五个 CLI 测试）。完整输出和图备份保存在忽略 Git 的 `logs/test/20261031-stale-setup-recovery/`。

## Setup 契约与前台生命周期（2026-10-31）

`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `python3 tests/clean_home.py` 均通过，完整输出保存在忽略 Git 的 `logs/test/20261031-setup-contract-lifecycle/`。Rust 测试通过 80 个单元测试和四个 CLI 测试。clean-home 路线确认：EOS 程序在 Apply 后发生变化时，Run 返回 HTTP 422，而前台 `isos serve`、图 API 和静态 IsosUI 仍可用。测试还覆盖 Apply、Run、Restart、MuJoCo 桩控制、部署停止及工作台 Ctrl+C 退出。

## 统一日志与源码布局（2026-10-31）

[源码布局](source-layout_cn.md) 现在将 CLI 命令、管理器协议和前台生命周期分开。内部模块、模板及测试文件按职责命名。旧进程日志 HTTP 路由已删除；统一 `logs` 路由默认使用 `[logging].default_source = "all"`。`max_bytes = 262144`、`backups = 2` 和 `max_sources = 32` 限制新增日志文件；超出来源文件上限的事件仍可从统一流读取。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`、`python3 tests/clean_home.py` 和 `python3 tests/service_manager.py` 均通过。完整输出位于忽略 Git 的 `logs/test/20261031-unified-logging-layout/`。

## 单一运行时与首次公开契约验证（2026-10-31）

IsOS 使用 `1.0.0` 图、`/api/isos/v1`、setup schema 1 和唯一的直接子进程管理器。EOS、EGIA 报告产品／图／setup v1；MuJoCo 包、EOS HTTP 控制接口和仿真器套接字保留各自独立的 v2 契约。已移除的 CLI／API 路由返回命令错误或 HTTP 404；旧图文件保留原样并给出转换指引。

本地复现使用 `ISOS_BIN=target/debug/isos`、`AETHER_BIN_DIR=../Island-eos/target/debug`、`ISOSUI_DIST_DIR=../IsosUI/dist/local` 和暂存的 `island/mujoco-sim@2.0.0` 包。结果保存在忽略 Git 的 `logs/test/20261031-single-runtime-v1/`：

- `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`、`python3 tests/test_install_user_service.py` 和 `python3 tests/service_manager.py` 均通过。
- `python3 tests/clean_home.py` 通过隔离的 Apply／Run／Restart／Stop、修订和软件包检查、HTTP 状态和日志、失败恢复及子进程清理。固定的五次启动和 200 次请求负载测得启动中位数 5845.438 毫秒、p99 5888.129 毫秒；HTTP 到 EOS 套接字中位数 10.165 毫秒、p99 10.544 毫秒、吞吐量 98.3 次／秒。
- `python3 tests/real_home.py` 通过真实 MuJoCo、显式请求的 GUI、Vision 取帧和来源切换、底盘和机械臂命令、重启、日志及子进程清理。本次启动为 6743.194 毫秒；20 帧取样的中位数为 374.239 毫秒、p99 400.717 毫秒、吞吐量 2.7 帧／秒。这些数值是本机基线，不是验收阈值。

## 重建后刷新已配置核心文件（2026-10-31）

对已应用的 v1 图，`isos init` 或 `isos config init` 校验已保存图和当前配置的核心程序，再原子重建核心 setup 文件及程序摘要。图内容和修订号保持不变。新草稿仍未应用。更换程序后执行 Run 或显式 Restart；`init` 不会重启运行中的部署。旧 `2.0.0` 图保持原样，需按[迁移指南](../maintenance/migration_cn.md)手动迁移。

检查命令：`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo build`，以及 `ISOS_BIN=target/debug/isos AETHER_BIN_DIR=../Island-eos/target/debug ISOSUI_DIST_DIR=../IsosUI/dist/local python3 tests/clean_home.py`。集成测试在 Apply 后修改已配置的 EOS 程序，确认就绪被阻止，运行 `isos init`，确认恢复就绪且图字节不变，然后验证 Run、Restart、Stop、失败恢复及子进程清理。完整输出保存在忽略 Git 的 `logs/test/20261031-init-core-refresh/`。

## 产品版本对齐（2026-10-31）

IsOS 报告 `0.1.0`，要求 EOS `0.2.0`，并识别 MuJoCo 软件包 `0.1.1`。图保持 `1.0.0`，IsOS API 保持 `/api/isos/v1`。`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `cargo build` 通过。`tests/clean_home.py` 和 `tests/real_home.py` 使用新 EOS 程序和暂存 MuJoCo 包通过，包括 Vision 帧、移动、重启和关闭。日志位于忽略 Git 的 `logs/test/20261031-version-alignment/`。
