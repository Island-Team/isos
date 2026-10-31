# Develop and verify the v2 manager

[中文](../../../docs_cn/isos/develop/backend-v2_cn.md)

The current chain is Rust IsOS, prebuilt Rust Aether executables from EOS, EGIA launchers, and a package-backed MuJoCo simulator owned by EOS. The generic starter graph comes from `templates/generic-graph-v2.toml` and contains no hardware package. `src/v2_starter.rs` fills local arguments; `src/core_setup.rs` calls each core's setup adapter; `src/v2_backend.rs` verifies packages; `src/v2_process.rs` supervises graph children; `src/services.rs` supervises explicitly started services outside the graph. `src/http.rs` serves the loopback API and configured static UI bundle on separate ports. Only `isos serve --ui-live` runs npm.

Build EOS, EGIA, and IsosUI in their own repositories. Set `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist` in `config/isos.toml`, then run `isos version check`. `isos build` and commit-based `isos version select` are retired from the v2 workflow. The starter's `isos.process.component@v1.1` nodes declare `role` and `args`; IsOS resolves the executable from the configured core directory. Apply checks the saved graph revision and writes generated core files atomically. Run and Restart verify those inputs again and probe each enabled core before launch.

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

The clean-home harness uses isolated `ISOS_HOME` and ports, a MuJoCo protocol stub, and configured EOS binaries. It checks setup-only init twice, API and UI lifecycle, graph validation/apply, start/restart, failure admission, logs, and Ctrl+C cleanup. The service harness uses a stub to check explicit start, status, idempotence, scoped stop, and root stop. Neither proves real MuJoCo physics or Island-models runtime behavior.

For a real simulator check, set all paths to existing artifacts:

```sh
ISOS_BIN=target/debug/isos \
ISOS_TEST_REAL_BUNDLE=/absolute/path/to/bundle \
ISOS_AETHER_BIN_DIR=../Island-eos/target/debug \
MUJOCO_ROOT=/absolute/path/to/mujoco \
ISOSUI_DIST_DIR=../IsosUI/dist/local \
python3 tests/real_home_v2.py
```

The real harness requires a compatible MuJoCo bundle and runtime; run it only when those dependencies are available. A product check must configure verified EOS, EGIA, and static UI artifacts, then serve from an isolated home without npm or source builds. Run `isos version check` before interpreting a live result. See the [quick start](../usage/get-started.md), [local API](../interfaces/local-api.md), and [service contract](../interfaces/services.md).

## Historical measurements

Earlier 2026-10-30 measurements and command transcripts in `logs/test/20261030-*/` used prior starter graphs and, in some runs, a foreground `isos init` or npm UI. They are historical baselines, not instructions for the current CLI. The manager redesign checks of 2026-10-31 are in `logs/test/20261031-manager/` and `logs/test/20261031-manager-services/`; at that point the real MuJoCo harness was syntax checked but not run because the bundle and `MUJOCO_ROOT` were unavailable in the workspace.

## Configured-build verification (2026-10-31)

With IsOS 2.0.0, EOS workspace binaries from `../Island-eos/target/debug`, IsosUI's `dist/local`, and MuJoCo 3.16.0 installed locally, the following checks passed. Full output is saved under ignored `logs/test/20261031-configured-core-final/`.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release
ISOS_BIN=target/debug/isos AETHER_BIN_DIR=../Island-eos/target/debug ISOSUI_DIST_DIR=../IsosUI/dist/local python3 tests/clean_home_v2.py
ISOS_BIN=target/debug/isos ISOS_AETHER_BIN_DIR=../Island-eos/target/debug ISOSUI_DIST_DIR=../IsosUI/dist/local ISOS_TEST_REAL_BUNDLE=../isos-mujoco-sim/logs/test/20261031-generic-device-preset/bundle MUJOCO_ROOT=/home/ashex/mujoco-3.16.0-linux-x86_64/mujoco-3.16.0 python3 tests/real_home_v2.py
```

The Rust suite passed 77 unit tests and five CLI tests. The mock route covered Apply, Run, restart, failure admission, and stop. The real route covered camera frames, base and manipulator commands, nested asset change rejection, restart, and Ctrl+C stop. On this host the real run measured startup 7.04 s and 20 captures at median 376.25 ms, p99 428.04 ms, and 2.6 requests/s. These are initial measurements for this host and scene, not performance targets. A separate checkout test verified that changing `aether-runtime` after Apply changes readiness to blocked and prevents Run from starting hardware.

## Saved setup status recovery (2026-10-31)

The saved MuJoCo graph was reapplied with `isos config apply` after the current EOS profile accepted its adapter family. A successful Apply now clears the previous setup failure; readiness identifies cores absent from the saved graph as not enabled. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed (78 unit tests and five CLI tests). Full output and the graph backup are in ignored `logs/test/20261031-stale-setup-recovery/`.
