# Local IsOS v2 API

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

`isos init --mode dev|product` creates configuration and a starter graph, then exits. `isos serve` starts the IsOS API on `127.0.0.1:16000` by default (`ISOS_HTTP_PORT` selects another loopback port), serves the selected static IsosUI build on `127.0.0.1:16001` (`ISOSUI_PORT` selects another port), and opens an owner-only Unix manager socket below `$XDG_RUNTIME_DIR/isos/` (or `ISOS_HOME/run/`). `isos serve --ui-live` supervises Webpack only in dev mode. The runtime directory is mode `0700`, and the manager socket is mode `0600`. The HTTP response envelope is `{schema_version:2,ok,data,error}`. The manager's one-line Unix envelope remains schema 1 internally.

The API binds only to `127.0.0.1` and serves no frontend files. IsosUI calls it directly by address and port without a URL or bearer token. The API `Host` must match its loopback address and configured port. Browser requests may use the UI origin on `127.0.0.1:16001` (or configured `ISOSUI_PORT`), the API's own origin, or the hosted IsosUI origins `https://www.os-island.com` and `https://os-island.com`; other Origins are rejected. Cross-origin preflight permits GET, POST, PUT, and Content-Type. Ctrl+C or `isos stop` shuts down both servers, deployments, services, and the owner-only manager.

| Route | Response or request | Purpose |
| --- | --- | --- |
| `GET /api/isos/v2/node-graph` | `{revision,graph}` | Read the single saved graph. A missing graph returns an editable empty v2 draft. |
| `PUT /api/isos/v2/node-graph` | `{schema_version:2,revision,graph}` | Validate and apply the graph. Stale revision returns 409. |
| `POST /api/isos/v2/node-graph/validate` | `{schema_version:2,graph}` → `{valid,diagnostics}` | Registry and package admission, without applying. |
| `GET /api/isos/v2/node-types` | `{types:[...]}` | Node type catalog from `aether-profile`. |
| `GET /api/isos/v2/node-options` | `{adapter_families,primitive_bindings,targets}` | Selector options for the saved graph. |
| `POST /api/isos/v2/node-options` | `{schema_version:2,graph}` → `{adapter_families,primitive_bindings,targets}` | Selector options for an unapplied draft. |
| `GET /api/isos/v2/build-binaries` | `{selected:receipt|null}` | Exact staged Rust EOS binary set. |
| `POST /api/isos/v2/build-binaries` | `{schema_version:2,target:"eos",install_deps:false}` → `{receipt,suggested_graph}` | Build and stage Rust EOS binaries without changing selection. Preview the suggested graph, then use the select endpoint. |
| `POST /api/isos/v2/build-binaries/select` | `{schema_version:2,selector,revision}` → `{receipt,graph:{revision,graph}}` | Select a verified staged EOS build and apply its three process commands only if the graph revision still matches. |
| `GET /api/isos/v2/node-sets` | `{sets:[{package,id,parameters,nodes}]}` | Installed package node sets. |
| `POST /api/isos/v2/node-sets/preview` | `{package:{id,version},set_id,graph,values:{name:string}}` → `{graph,added_paths}` | Compose a set into a draft without saving it; invalid composition returns 422. |
| `GET /api/isos/v2/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | Installed/linked v2 backend choices and prerequisite checks. |
| `POST /api/isos/v2/packages/link` | `{id,version,path,manifest?}` | Link a local bundle; `manifest` is an optional absolute v4 release JSON path. If omitted, IsOS reads `<path>/release.json` when present. |
| `GET /api/isos/v2/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home setup summary. |
| `POST /api/isos/v2/process-control` | `{schema_version:2,operation,revision?}` | `run`, `start`, `stop`, or `restart`. The optional revision prevents acting on a stale editor view. |
| `GET /api/isos/v2/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | Pinned package and direct-child status. |
| `GET /api/isos/v2/process-logs/{node}?lines=100` | `{node,lines}` | Recent stdout/stderr lines, capped at 1000. |
| `GET /api/isos/v2/logs?source=all&cursor=...&limit=100` | `{lines:[{timestamp,level,source,event_code,message,pid}],next_cursor,rotated}` | Read the unified rotating JSON stream; `source=isos` and process sources remain available. At most 200 lines or 64 KiB per call. `cursor` is opaque; `rotated` signals a replaced file. The UI severity selector filters display only. |
| `GET /api/isos/v2/version` | `{api_schema_version,isos_version}` | Workbench compatibility check. |

The process manager validates the saved graph and exact package again before starting children. EOS starts MuJoCo as its direct child and stops it after admitted actuators are stopped. Graph layout belongs to the editor; the HTTP graph contains runtime configuration and typed connections only.

## Owner-only manager operations

The Unix socket accepts one JSON request per line: `{schema_version:1,op,deployment?,revision?,service_id?,cursor?,limit?}`. It replies `{schema_version:1,ok,data,error}`. The manager checks peer UID before processing. `manager_identity` returns `pid` (process ID), `start_ticks` (Linux process start identity), `mode` (`dev` or `product`), `executable` (absolute path), `executable_sha256` (digest), IsOS/API/service-contract versions, `api_origin`, and `ui_url`. Use this identity to distinguish an existing IsOS owner from a foreign port listener; a port number alone never confers ownership.

`manager_shutdown` stops all graph children and independent services, then ends the API, UI, and manager. `v2_stop` affects only the requested deployment. `services_list`, `services_validate`, `services_status`, `services_start`, `services_stop`, `services_restart`, and `services_logs` operate on the [independent service definitions](services.md); start/stop/restart/logs require `service_id`, and logs accepts opaque `cursor` plus `limit` 1–200. A CLI with a different executable digest may still request shutdown but cannot start or restart deployment or services. `isos status --json` combines deployment status, manager identity, and service statuses; `isos version check` compares configured, selected, and active identities.

Log `source` may be `all`, `isos`, `services`, a service source such as `service-gateway`, `packages`, `graph`, `ui`, `eos`, `egia`, `mujoco`, `bridge`, `config`, or a validated child ID. `limit` is 1–200 (default 100). Each line has an RFC 3339 UTC `timestamp`, a `level` (including structured child severity where available), `source`, `message`, and optional `event_code` and `pid`. Unstructured child stderr is recorded as `warn`. `next_cursor` is a string even when no more lines are available; poll with it for later writes. Files have a `0700` directory and `0600` regular-file permissions, rotate after `[logging].max_bytes` bytes (default 1 MiB), and retain `[logging].backups` copies (default 3). The legacy process endpoint reads mirrored raw lines.

`[logging].level` filters file records at or above `trace < debug < info < warn < error` (default `info`). Unstructured child stdout is `info` and stderr is `warn`; structured child severity takes precedence. The UI severity selector filters displayed entries without changing recording. Terminal status and command errors are unaffected.

When an applied graph has a MuJoCo package, IsOS passes the deployment log directory to EOS runtime. EOS alone writes and rotates `mujoco.log` using the same JSON line and permission contract; IsOS reads it through this API.

### Local runtime configuration

IsOS reads `~/.config/isos/config.toml` (or `ISOS_HOME/config/config.toml`) if present. Set `ISOS_CONFIG` to an absolute path to require another file. Environment variables override matching TOML fields. The command working directory is not searched for `.env`. See [config.example.toml](../../../config.example.toml).

| TOML field | Type / unit | Purpose and validation | Environment override |
| --- | --- | --- | --- |
| `server.http_port` | integer TCP port | IsOS loopback API; 1024–65535 | `ISOS_HTTP_PORT` |
| `server.ui_port` | integer TCP port | IsosUI loopback frontend; 1024–65535, different from HTTP port | `ISOSUI_PORT` |
| `paths.ui_repo` | absolute path string | IsosUI source repository for builds or explicit `--ui-live`; npm dependencies are required only for live mode | `ISOSUI_REPO_PATH` |
| `paths.aether_bin_dir` | absolute path string | Aether Rust executable directory | `ISOS_AETHER_BIN_DIR` |
| `paths.mujoco_root` | absolute path string | Optional MuJoCo runtime root | `MUJOCO_ROOT` |
| `deployment.name` | slug string | Default graph/log namespace; validated deployment name | `ISOS_DEPLOYMENT` |
| `logging.max_bytes` | integer bytes | Rotation threshold, 65,536–100,000,000; default 1,048,576 | `ISOS_LOG_MAX_BYTES` |
| `logging.backups` | integer count | Rotated copies, 1–10; default 3 | `ISOS_LOG_BACKUPS` |
| `logging.level` | enum string | Minimum file severity: `trace`, `debug`, `info`, `warn`, or `error`; default `info` | `ISOS_LOG_LEVEL` |

## Rust binary and selector contracts

`receipt` contains `schema_version` (integer 2), `component` (`eos`), `product_version` (`2.0.0`), `source_commit` (full Git SHA), `source_dirty` (boolean indicating modified or untracked source), `source_fingerprint` (SHA-256 of commit and source files), `source_root` (optional absolute source path for development drift checks), `binaries` (absolute paths keyed by executable name), and `sha256` (matching file digests). Build stages the five Aether executables together under the private IsOS data directory without changing selection. Product mode admits only clean fingerprinted receipts. Run probes the exact EOS, bridge, config, and profile executable capabilities and returns an error when the set is stale or the graph points elsewhere. `suggested_graph` is `{revision,graph}` or null and is never applied automatically. CLI `isos build eos` calls the same build service; the select endpoint or `isos version select eos <selector>` checks the graph revision before applying the three executable paths and pin. The selector accepts a unique commit prefix, product version, or source fingerprint prefix; use the fingerprint when builds share a commit.

IsOS and IsosUI builds use separate immutable receipts with the same source identity fields. `isos build isos` stages a release executable; `isos version select isos <commit>` requires a stopped manager before activating it. An IsosUI receipt additionally binds a directory digest to its manifest; product serving reads only the selected, verified directory.

`adapter_families` entries contain `id` (stable adapter ID), `device_kinds` (device classes), `package` (optional exact `{id,version}`), `compiled` and `available` (booleans), `transport` and `description` (strings), optional `supported_primitives`, and `reason` when unavailable. `primitive_bindings` entries contain `node_type`, `device_kind`, `movement_types`, `available`, `reason`, and `primitives` with exact name, version, movement type, and description. `targets` entries contain the four-segment `path`, `node_type`, `device_kind`, `adapter_type`, `movement_types`, `available`, and `reason`. `GET /node-types` also forwards EOS `primitive_specs` and typed `data_types`.
