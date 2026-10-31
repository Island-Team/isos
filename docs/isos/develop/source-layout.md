# IsOS source layout

[中文](../../../docs_cn/isos/develop/source-layout_cn.md)

`src/main.rs` declares the owned modules and calls `app/entry.rs`. Add behavior to its owner:

| Directory | Responsibility |
| --- | --- |
| `src/app/` | CLI, loopback HTTP API, foreground workbench, runtime configuration, authentication, and verified UI bundle. It calls domain and runtime interfaces but does not supervise graph children. |
| `src/domain/` | Saved graph starter, node sets and options, packages, releases, backend descriptors, and independent services. Package installation does not start a deployment. |
| `src/runtime/` | Direct-child manager and socket protocol, graph process projection and supervision, core setup, executable preflight, readiness, locks, and bounded structured logs. |

The saved `<name>.nodes.json` graph is the only deployment graph. `src/runtime/lock.rs` serializes graph, generated configuration, and package mutations. The old component graph, pinned-build checks, and systemd child runtime were removed after their callers migrated. The optional `isos.service` runs the same `isos serve` command as the foreground operator flow.

The unified log API is `GET /api/isos/v1/logs`. `config/isos.toml` sets its default source and retention limits. New source files stop at `logging.max_sources`; events from additional sources remain in the bounded unified stream.
