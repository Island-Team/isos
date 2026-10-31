# IsOS source layout

[中文](../../../docs_cn/isos/develop/source-layout_cn.md)

`src/main.rs` initializes configuration, parses the command, and dispatches it. New behavior belongs with the owner below:

| Path | Responsibility |
| --- | --- |
| `src/cli/mod.rs`, `src/cli/commands.rs` | Public command syntax and command handlers. CLI handlers call domain modules and the manager client; they do not own child processes. |
| `src/manager/mod.rs`, `src/manager/protocol.rs` | Owner-only socket protocol, deployment and service request dispatch, and manager client calls. Protocol fields are bounded and validated before state changes. |
| `src/foreground.rs` | Workbench UI, browser launch, foreground process lifetime, and signal handling. |
| `src/config.rs`, `src/core_setup.rs`, `src/readiness.rs` | Runtime paths and settings, core setup files, and launch readiness. |
| `src/process.rs`, `src/backend.rs`, `src/starter.rs`, `src/core_bins.rs` | Graph process supervision, verified backend inputs, starter graph arguments, and configured core binary paths. |
| `src/log_store.rs`, `src/manager_log.rs` | Bounded structured log storage and manager event capture. |
| `src/http.rs` | Loopback HTTP API and static UI server. It calls manager and domain interfaces; it does not supervise children. |
| `src/packages.rs`, `src/services.rs`, `src/auth.rs`, `src/release.rs`, `src/node_sets.rs`, `src/node_options.rs`, `src/isosui.rs` | Package, service, account, release, graph catalog, and UI bundle domains. |

`component.rs`, `graph.rs`, and `versions.rs` still serve legacy component commands. Remove them when the last command caller is replaced and equivalent tests pass. Keep version numbers in public graph, package, and wire contracts; internal module and test filenames describe their role.

The unified log API is `GET /api/isos/v2/logs`. `config/isos.toml` sets its default source and retention limits. New source files stop at `logging.max_sources`; events from additional sources remain in the bounded unified stream.
