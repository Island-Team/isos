# IsOS configuration, version, and log contract

[中文](../../../docs_cn/isos/interfaces/configuration_cn.md)

**Status:** The XDG/`ISOS_HOME` roots, `config/isos.toml`, schema-1 local API, rotating source logs, and `isos version check` exist today. The generated snapshot layout, cross-core event fields, and supported-version arrays below are the **0.x target**. See [current API fields](local-api.md#local-runtime-configuration) and [first-use workflow](../usage/get-started.md).

## Paths and precedence

`ISOS_HOME` must be absolute. When set, `<base>/config`, `<base>/data`, and `<base>/run` are the three roots. Otherwise IsOS uses `${XDG_CONFIG_HOME:-~/.config}/isos`, `${XDG_DATA_HOME:-~/.local/share}/isos`, and `${XDG_RUNTIME_DIR:-/run/user/<uid>}/isos`. IsOS owns these paths, uses owner-only directories (`0700`) and private files (`0600`), and never treats the command working directory as configuration.

| Relative path and owner | Kind / purpose | Write and validation rule |
| --- | --- | --- |
| `config/deployments/<name>.nodes.json` / IsOS | Authored deployment graph | `name` is a validated slug; revision is SHA-256 of saved bytes; only revision-checked Apply or explicit migration writes it. |
| `config/services/<id>.toml` / IsOS | Independently started model-like service | Explicit service command only; never a graph node in this release. |
| `config/mode` / IsOS | `dev` or `product` mode | `isos init` writes; invalid values fail startup. |
| `data/generated/<name>/<revision>/eos/` and `egia/` / IsOS | Validated core setup snapshot | **Target:** immutable directory named by saved graph revision, with `core-setup/launch-inputs.json` and owner overlays; write temporary files, fsync, then atomically publish. `0.1.0` Run/Restart select it only by the current saved-byte digest and verify its manifest. The existing flat per-deployment generated directory remains for `1.0.0` compatibility only. |
| `data/packages/<namespace>/<name>/<version>/` / IsOS | Verified exact package release, receipt, node-type sidecar | Installation or link creates a receipt; never activates a graph. Linked source changes are re-fingerprinted at Apply and Run. |
| `data/logs/<name>/` / IsOS and EOS child | Rotating JSONL source files and unified `events.log` | Owner-only; cursor reads bounded; an active run ID appears in target events. |
| `data/cache/` / IsOS | Download and catalog cache | **Target:** disposable; identity comes from verified receipt and digest, not cache presence. |
| `run/manager.sock`, `run/<name>-*.sock` / owning process | Private manager and core sockets | UID checked; remove stale sockets only after owner identity check. |

`ISOS_CONFIG` points to an absolute TOML file when set; otherwise IsOS reads its checkout `config/isos.toml`. For each supported TOML field, an explicitly set environment variable wins, then TOML, then the documented default. Relative source-artifact paths in TOML resolve from the IsOS checkout. `ISOS_HOME`/XDG select private state roots independently. Never store secrets in graph settings, generated snapshots, receipts, logs, or UI preferences. Service `env_from` imports a named process environment variable at launch; status prints the name, never its value. A missing required secret fails preflight without echoing it.

## Version admission

| Identity | Target value or rule | Compatibility meaning |
| --- | --- | --- |
| Product releases | IsOS/EGIA/IsosUI `0.1.0`, EOS `0.2.0`, MuJoCo package `0.1.1` | No product is declared stable `1.0.0`. |
| Authored graph writer | `0.1.0` | Reader also accepts existing `1.0.0` saved graphs; original bytes stay unchanged. |
| New node type ID | `@v0.1` | Legacy `@v1.0`/`@v1.1` IDs retain their registered safe behavior. |
| HTTP API path and setup envelope | `/api/isos/v1`, `schema_version:1` | Stable compatibility identity; not product release number. |
| Package release manifest | `schema_version:4` | Distribution format; exact package version is separate. |
| MuJoCo socket protocol | `2.0.0` | Existing simulator wire identity; EOS and package must agree. |

**Target algorithm:** Probe each enabled core's setup `identity` without starting it; read `supported_graph_versions`, `supported_node_type_versions`, setup schema, and product version; compare the actual saved graph and exact pinned package contributions. Validate package receipts/digests and compiled adapter availability. Report a supported-version list and one actionable reason for each mismatch in `isos version check --json`. Apply and Run repeat the check against current bytes and executables. A changed package, graph, or executable invalidates the generated snapshot. Never reinterpret a `1.0.0` graph as `0.1.0` to pass a check. Current identity exposes single `graph_version` and `node_type_version` strings; arrays are target additions.

## Target shared JSONL event

Each module logs one `info` event when it becomes ready, then state transitions, warnings and failures; periodic health produces a log only on change or a bounded summary interval. Full child output remains in its owner's log store. IsOS's unified stream receives bounded, sanitized events. Logging must not stall actuator stop or process shutdown.

| Field | JSON type / unit | Purpose, default, validation, compatibility |
| --- | --- | --- |
| `timestamp` | RFC 3339 UTC string / ms | Required event time; preserve existing field. |
| `level` | `trace`, `debug`, `info`, `warn`, `error` / — | Required severity; existing filter order stays. |
| `source` | string / source ID | Required 1–48 ASCII letters, digits, `_` or `-`; identifies emitter. |
| `core` | string / core ID | Required runtime owner (`isos`, `eos`, or `egia`); MuJoCo child events use `eos` with `source:mujoco`. |
| `node_path` | string or null / canonical path | Optional owner node; null or absent for a global event. |
| `event_code` | string / machine code | Required stable lowercase snake case, 1–64 characters; existing optional readers still accept it. |
| `run_id` | string / opaque ID | Required owner session identity; IsOS uses a manager-session ID before deployment starts and a deployment-run ID thereafter, and attaches one when normalizing legacy `log_append`. |
| `correlation_id` | string or null / opaque ID | Optional request correlation; no secret or user text embedded. |
| `message` | string / UTF-8 bytes | Required human explanation, max 8192 bytes, redacted before write. |
| `fields` | object / bounded metadata | Optional scalar values only, maximum 16 keys and 4 KiB encoded; deny secrets, paths with credentials, and unbounded payloads. |
| `pid` | integer or null / process ID | Optional existing process metadata. |

```json
{"timestamp":"2026-11-01T10:00:00.000Z","level":"info","source":"eos","core":"eos","node_path":null,"event_code":"runtime_ready","run_id":"r-7c2a","correlation_id":null,"message":"EOS runtime ready","fields":{"state":"ready"},"pid":4242}
```

Read with `isos logs all --level info` or `GET /api/isos/v1/logs?source=all&limit=100`; use `next_cursor` to poll. One read is limited to 200 lines or 64 KiB; `rotated:true` means the cursor file was replaced and the UI must show that gap. Existing defaults are 256 KiB per file, two backups, 32 source files, and `info` minimum. New source files beyond that cap still appear in the unified stream. Never infer readiness from the startup log alone: `isos status --json` is authoritative live state.

**Target ingestion algorithm:** validate source/level/length, attach the authenticated owner and run context, remove secret keys and credential-like values, bound fields, write one JSON line to source and unified streams, rotate under a lock, and return without blocking on a slow UI reader. A dropped or rejected event increments an observable counter; warning emission is rate-limited to avoid recursion. HTTP and CLI cursors apply display filtering after storage. Existing `log_append` remains a small owner-only compatibility entry point; future cross-core transport may add fields without requiring old callers to send them.
