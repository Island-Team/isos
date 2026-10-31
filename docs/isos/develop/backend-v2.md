# Develop and verify the v2 manager

[中文](../../../docs_cn/isos/develop/backend-v2_cn.md)

The current chain is Rust IsOS, prebuilt Rust Aether executables from EOS, EGIA launchers, and a package-backed MuJoCo simulator owned by EOS. The generic starter graph comes from `templates/generic-graph.toml` and contains no hardware package. `src/starter.rs` fills local arguments; `src/core_setup.rs` calls each core's setup adapter; `src/backend.rs` verifies packages; `src/process.rs` supervises graph children; `src/services.rs` supervises explicitly started services outside the graph. `src/http.rs` serves the loopback API and configured static UI bundle on separate ports. Only `isos serve --ui-live` runs npm.

Build EOS, EGIA, and IsosUI in their own repositories. Set `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist` in `config/isos.toml`, then run `isos version check`. `isos build` and commit-based `isos version select` are retired from the v2 workflow. The starter's `isos.process.component@v1.1` nodes declare `role` and `args`; IsOS resolves the executable from the configured core directory. Apply checks the saved graph revision and writes generated core files atomically. Run and Restart verify those inputs again and probe each enabled core before launch.

## Repeatable checks

From a clean source checkout, run the repository checks and save complete output under ignored `logs/test/<run-id>/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
ISOS_BIN=target/debug/isos AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" python3 tests/clean_home.py
ISOS_BIN=target/debug/isos python3 tests/service_manager.py
```

The clean-home harness uses isolated `ISOS_HOME` and ports, a MuJoCo protocol stub, and configured EOS binaries. It checks setup-only init twice, API and UI lifecycle, graph validation/apply, start/restart, failure admission, logs, and Ctrl+C cleanup. The service harness uses a stub to check explicit start, status, idempotence, scoped stop, and root stop. Neither proves real MuJoCo physics or Island-models runtime behavior.

For a real simulator check, set all paths to existing artifacts:

```sh
ISOS_BIN=target/debug/isos \
ISOS_TEST_REAL_BUNDLE=/absolute/path/to/bundle \
ISOS_AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" \
MUJOCO_ROOT=/absolute/path/to/mujoco \
ISOSUI_DIST_DIR="$(realpath ../IsosUI/dist/local)" \
python3 tests/real_home.py
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
ISOS_BIN=target/debug/isos AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" ISOSUI_DIST_DIR="$(realpath ../IsosUI/dist/local)" python3 tests/clean_home.py
ISOS_BIN=target/debug/isos ISOS_AETHER_BIN_DIR="$(realpath ../Island-eos/target/debug)" ISOSUI_DIST_DIR="$(realpath ../IsosUI/dist/local)" ISOS_TEST_REAL_BUNDLE=../isos-mujoco-sim/logs/test/20261031-generic-device-preset/bundle MUJOCO_ROOT=/home/ashex/mujoco-3.16.0-linux-x86_64/mujoco-3.16.0 python3 tests/real_home.py
```

The Rust suite passed 77 unit tests and five CLI tests. The mock route covered Apply, Run, restart, failure admission, and stop. The real route covered camera frames, base and manipulator commands, nested asset change rejection, restart, and Ctrl+C stop. On this host the real run measured startup 7.04 s and 20 captures at median 376.25 ms, p99 428.04 ms, and 2.6 requests/s. These are initial measurements for this host and scene, not performance targets. A separate checkout test verified that changing `aether-runtime` after Apply changes readiness to blocked and prevents Run from starting hardware.

## Saved setup status recovery (2026-10-31)

The saved MuJoCo graph was reapplied with `isos config apply` after the current EOS profile accepted its adapter family. A successful Apply now clears the previous setup failure; readiness identifies cores absent from the saved graph as not enabled. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` passed (78 unit tests and five CLI tests). Full output and the graph backup are in ignored `logs/test/20261031-stale-setup-recovery/`.

## Setup contract and foreground lifecycle (2026-10-31)

`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `python3 tests/clean_home.py` passed with output in ignored `logs/test/20261031-setup-contract-lifecycle/`. The Rust suite passed 80 unit tests and four CLI tests. The clean-home route verified that a changed EOS binary rejects Run with HTTP 422 while the foreground `isos serve` process, graph API, and static IsosUI remain available. It also covered Apply, Run, Restart, mock MuJoCo controls, deployment stop, and workbench Ctrl+C shutdown.

## Unified logging and source layout (2026-10-31)

The [source layout](source-layout.md) now separates CLI commands, manager protocol, and foreground lifecycle. Internal module, template, and test harness filenames use descriptive names. The legacy process-log HTTP route has been removed; the unified `logs` route defaults to `[logging].default_source = "all"`. `max_bytes = 262144`, `backups = 2`, and `max_sources = 32` limit new log files; a source beyond that count remains available through the unified stream. `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `python3 tests/clean_home.py`, and `python3 tests/service_manager.py` passed. Complete output is in ignored `logs/test/20261031-unified-logging-layout/`.

## Single-runtime and first-public-contract verification (2026-10-31)

IsOS uses the `1.0.0` graph, `/api/isos/v1`, setup schema 1, and one direct-child manager. EOS and EGIA report product/graph/setup v1; the MuJoCo package, EOS HTTP control API, and simulator socket retain their independent v2 contracts. The removed CLI/API routes return command errors or HTTP 404, and obsolete graph files are left in place with conversion guidance.

The reproducible local checks used `ISOS_BIN=target/debug/isos`, `AETHER_BIN_DIR=../Island-eos/target/debug`, `ISOSUI_DIST_DIR=../IsosUI/dist/local`, and the staged `island/mujoco-sim@2.0.0` bundle. Results are in ignored `logs/test/20261031-single-runtime-v1/`:

- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build`, `python3 tests/test_install_user_service.py`, and `python3 tests/service_manager.py` passed.
- `python3 tests/clean_home.py` passed the isolated Apply/Run/Restart/Stop flow, revision and package checks, HTTP status/logs, failure recovery, and child shutdown. The fixed five-start and 200-request workload measured startup median 5845.438 ms, p99 5888.129 ms; HTTP-to-EOS socket median 10.165 ms, p99 10.544 ms, throughput 98.3 requests/s.
- `python3 tests/real_home.py` passed the real MuJoCo route, explicitly requested GUI, vision gate capture and source switching, base and arm dispatch, restart, logs, and child shutdown. In this run, startup was 6743.194 ms and 20 captures measured median 374.239 ms, p99 400.717 ms, throughput 2.7 frames/s. These numbers are local baselines, not acceptance thresholds.
