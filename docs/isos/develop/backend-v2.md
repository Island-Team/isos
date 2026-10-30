# Develop and verify the v2 manager

[中文](../../../docs_cn/isos/develop/backend-v2_cn.md)

The current chain is Rust IsOS, selected Rust Aether executables, and a package-backed MuJoCo simulator owned by EOS. The generic starter graph comes from `templates/generic-graph-v2.toml` and contains no hardware package. `src/v2_starter.rs` fills local paths; `src/v2_build.rs` stages and selects exact EOS receipts; `src/v2_backend.rs` verifies packages; `src/v2_process.rs` supervises graph children; `src/services.rs` supervises explicitly started services outside the graph. `src/http.rs` serves the loopback API and selected static UI on separate ports. Only `isos serve --ui-live` runs npm.

## Repeatable checks

From a clean source checkout, run the repository checks and save complete output under ignored `logs/test/<run-id>/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
ISOS_BIN=target/debug/isos AETHER_BIN_DIR=../Island-eos/target/debug python3 tests/clean_home_v2.py
ISOS_BIN=target/debug/isos python3 tests/service_manager_v2.py
```

The clean-home harness uses isolated `ISOS_HOME` and ports, a MuJoCo protocol stub, and explicit EOS build/select. It checks setup-only init twice, API and UI lifecycle, graph validation/apply, start/restart, failure admission, logs, and Ctrl+C cleanup. The service harness uses a stub to check explicit start, status, idempotence, scoped stop, and root stop. Neither proves real MuJoCo physics or Island-models runtime behavior.

For a real simulator check, set all paths to existing artifacts:

```sh
ISOS_BIN=target/debug/isos \
ISOS_TEST_REAL_BUNDLE=/absolute/path/to/bundle \
ISOS_AETHER_BIN_DIR=../Island-eos/target/debug \
MUJOCO_ROOT=/absolute/path/to/mujoco \
ISOSUI_REPO_PATH=../IsosUI \
python3 tests/real_home_v2.py
```

The real harness requires a compatible MuJoCo bundle and runtime; run it only when those dependencies are available. A product check must select clean IsOS, EOS, and static UI receipts, then serve from an isolated home without npm or source checkouts. Run `isos version check` before interpreting a live result. See the [quick start](../usage/get-started.md), [local API](../interfaces/local-api.md), and [service contract](../interfaces/services.md).

## Historical measurements

Earlier 2026-10-30 measurements and command transcripts in `logs/test/20261030-*/` used prior starter graphs and, in some runs, a foreground `isos init` or npm UI. They are historical baselines, not instructions for the current CLI. The manager redesign checks of 2026-10-31 are in `logs/test/20261031-manager/` and `logs/test/20261031-manager-services/`; at that point the real MuJoCo harness was syntax checked but not run because the bundle and `MUJOCO_ROOT` were unavailable in the workspace.
