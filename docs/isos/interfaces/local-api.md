# Local IsOS v1 API

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

**Implementation status:** The route table and first compatibility-reader slice describe implemented behavior. The setting-options and supported-version identity sections remain target contracts. The `/api/isos/v1` path and schema-1 envelope are compatibility identities, not a product `1.0.0` declaration.

`isos init --mode dev|product` creates configuration and a starter graph when absent. If the graph was previously applied, it validates the saved v1 graph and refreshes generated core configuration and executable digests from the configured binary directories; it never edits the graph or starts processes. An incompatible saved graph or core stops the refresh with guidance, leaving the graph untouched. `isos serve` starts the IsOS API on `127.0.0.1:16000` by default (`ISOS_HTTP_PORT` selects another loopback port), serves the configured prebuilt IsosUI bundle on `127.0.0.1:16001` (`ISOSUI_PORT` selects another port), and opens an owner-only Unix manager socket below `$XDG_RUNTIME_DIR/isos/` (or `ISOS_HOME/run/`). `isos serve --ui-live` supervises Webpack only in dev mode. The runtime directory is mode `0700`, and the manager socket is mode `0600`. The HTTP response envelope is `{schema_version:1,ok,data,error}`. The manager's one-line Unix envelope remains schema 1 internally.

The API binds only to `127.0.0.1` and serves no frontend files. IsosUI calls it directly by address and port without a URL or bearer token. The API `Host` must match its loopback address and configured port. Browser requests may use the UI origin on `127.0.0.1:16001` (or configured `ISOSUI_PORT`), the API's own origin, or the hosted IsosUI origins `https://www.os-island.com` and `https://os-island.com`; other Origins are rejected. Cross-origin preflight permits GET, POST, PUT, and Content-Type. Ctrl+C or `isos stop` shuts down both servers, deployments, services, and the owner-only manager.

| Route | Response or request | Purpose |
| --- | --- | --- |
| `GET /api/isos/v1/node-graph` | `{revision,graph}` | Read the single saved graph. A missing graph returns an editable empty v1 draft. |
| `PUT /api/isos/v1/node-graph` | `{schema_version:1,revision,graph}` | Validate and apply the graph. Stale revision returns 409. |
| `POST /api/isos/v1/node-graph/migration/preview` | `{schema_version:1,revision}` → `{revision,proposed_revision,graph,changes,can_apply,diagnostics}` | Preview the saved legacy graph; no writes. |
| `POST /api/isos/v1/node-graph/migration/apply` | `{schema_version:1,revision,proposed_revision}` → `{revision,graph,active_revision}` | Commit exactly the previewed migration under revision checks. |
| `POST /api/isos/v1/node-graph/validate` | `{schema_version:1,graph}` → `{valid,diagnostics}` | Registry and package admission, without applying. Exact registered port type mismatches return `edge_type_mismatch` with a source, target, input and JSON-pointer path to the input source. |
| `GET /api/isos/v1/node-types` | `{types:[...]}` | Node type catalog from `aether-profile`. |
| `GET /api/isos/v1/core-setup/{role}/catalog` | Setup reply with `catalog` | Query the configured `eos` or `egia` adapter without starting its runtime. |
| `POST /api/isos/v1/core-setup/{role}/validate` | `{schema_version:1,graph,revision?}` → setup reply | Validate an editor draft with a core's own rules, without saving it. |
| `POST /api/isos/v1/core-setup/{role}/render` | `{schema_version:1,graph,revision?}` → setup reply | Preview generated core configuration; the API does not write the preview. |
| `GET /api/isos/v1/node-options` | `{adapter_families,primitive_bindings,targets}` | Selector options for the saved graph. |
| `POST /api/isos/v1/node-options` | `{schema_version:1,graph}` → `{adapter_families,primitive_bindings,targets}` | Selector options for an unapplied draft. |
| `GET /api/isos/v1/core-readiness` | `{cores,graph,isos,isosui}` | Inspect configured core paths, compatibility, whether each core is enabled in the saved graph, graph configuration, configured endpoint, and running state. The endpoint describes configuration, not an active listener. |
| `GET /api/isos/v1/node-sets` | `{sets:[{package,id,parameters,nodes}]}` | Installed package node sets. |
| `POST /api/isos/v1/node-sets/preview` | `{package:{id,version},set_id,graph,values:{name:string}}` → `{graph,added_paths}` | Compose a set into a draft without saving it; invalid composition returns 422. |
| `GET /api/isos/v1/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | Installed/linked v2 backend choices and prerequisite checks. |
| `POST /api/isos/v1/packages/link` | `{id,version,path,manifest?}` | Link a local bundle; `manifest` is an optional absolute v4 release JSON path. If omitted, IsOS reads `<path>/release.json` when present. |
| `GET /api/isos/v1/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home setup summary. |
| `POST /api/isos/v1/process-control` | `{schema_version:1,operation,revision?}` | `run`, `start`, `stop`, or `restart`. The optional revision prevents acting on a stale editor view. |
| `GET /api/isos/v1/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | Pinned package and direct-child status. |
| `GET /api/isos/v1/logs?source=all&cursor=...&limit=100&level=info` | `{lines:[{timestamp,level,source,event_code,message,pid}],next_cursor,rotated}` | Read the unified rotating JSON stream, filtering entries at or above `level`. At most 200 lines or 64 KiB per call. `cursor` is opaque; `rotated` signals a replaced file. |
| `GET /api/isos/v1/version` | `{api_schema_version,isos_version}` | Workbench compatibility check. |

For a draft with a registered port type mismatch, validation returns `{"valid":false,"diagnostics":[{"path":"/cores/eos/layers/control/groups/manual/nodes/target/inputs/status/0","code":"edge_type_mismatch","message":"eos/control/manual/source.out (base_observation) cannot feed eos/control/manual/target.status (status); compare the registered data type and its unit schema","source":"eos/control/manual/source","target":"eos/control/manual/target","input":"status"}]}` inside the normal schema-1 success envelope. `path` is a JSON pointer to the offending input source in this legacy `1.0.0` example (the legacy validator adds `/0` for a single source object); `source` and `target` are canonical four-segment paths. Type IDs must match exactly. Units live in the registered data type schema's `x-unit` field annotations for display; field-value unit validation remains target work, and no unit conversion is inferred. New `0.1.0` edge admission returns the actual source pointer and stable edge code with source, target, input, and declared data types when available; unrelated legacy validator errors can still use `/` and `invalid_graph`. Apply rejects the same mismatch with HTTP 422; it does not change the saved graph.

**G06 response extension.** On a valid `0.1.0` draft, Validate adds `edges:[{path,source,target,input,route_class,fixed,qos_eligible,routed}]`, sorted by each edge's actual input-source JSON Pointer. `route_class` is `eos_local`, `egia_local`, `service_bridge`, or `design_only`; `fixed` marks a safety, command, result, health, or status path that rejects user QoS; `qos_eligible` is true only for the exact EOS camera → gate route in M1. `routed` means an owner declares a delivery route, not that a live node is healthy. EOS `FixedSafety` maps to `route_class:eos_local,fixed:true,qos_eligible:false,routed:false` for graph inspection; the active Dispatcher route remains governed separately. Invalid edges return `valid:false` and path-specific diagnostics, without claiming a partial route list. For new graph validation, a single source object uses its object pointer with no synthetic `/0`; an array source uses its numeric index. Apply, migration preflight, Run, and Restart use the same edge admission and preserve saved bytes and active revision on rejection. The existing schema-1 envelope and legacy `1.0.0` response remain compatible.

Each edge also reports `data_type` (exact registered type ID), `transport` (exact registered transport), `priority` (integer `-20..19`, default `0`), and optional `frequency_hz` (finite positive Hz; absent means uncapped). These are declarations for inspection, not measured rates. Edge errors report the actual `path`, stable `code`, `message`, and, when known, `source`, `target`, `input`, `source_data_type`, and `target_data_type`. CLI errors print the same structured diagnostic as JSON when these extra fields exist.

### First compatibility-reader slice — implemented

`GET /node-graph` keeps the schema-1 envelope and returns `data:{revision,graph,read_only,diagnostics}`. The revision is SHA-256 of the exact saved bytes. An applied safe `1.0.0` graph is read-only until explicit migration; an unapplied legacy starter remains editable. A missing graph returns an editable `0.1.0` draft with `revision:""`; Init creates and saves the same new-version starter only when absent. A syntactically valid unknown version, such as `2.0.0`, returns the original graph, original-byte revision, `read_only:true`, and a `/version` `unsupported_graph_version` diagnostic. Malformed or oversized files return HTTP 422. GET never rewrites saved bytes.

`POST /node-graph/validate`, PUT Apply, CLI Apply, core setup, Run, and Restart admit only exact supported versions and approved type identities at their registered placement. An unknown version fails at `/version`; an unknown or legacy-only type fails at its `/node_type`; an unsafe process fails at its exact node or `settings/command`. A `0.1.0` Apply requires an explicit expected revision, owner-reader support, verified package inputs, and graph-wide core and placement admission. Validation returns diagnostics in `data.diagnostics`; Apply returns HTTP 422 for admission failures, 409 for stale revisions. A safe saved legacy graph submitted with the matching revision and structurally identical JSON returns the original revision without writing; changed applied legacy graphs require explicit migration. Preview and commit compare saved revisions under the graph lock, publish only after package and owner setup checks, and leave the active revision unchanged until Restart.

The [`graph compatibility fixtures`](../../../tests/fixtures/graph_compat/README.md) fix the expected paths and unchanged-byte behavior. The same version/type failures must appear through the CLI and manager preflight; GET remains available for inspection when admission fails.

The process manager validates the saved graph and exact package again before starting children. EOS starts MuJoCo as its direct child and stops it after admitted actuators are stopped. Graph layout belongs to the editor; the HTTP graph contains runtime configuration and typed connections only.

### On-demand core setup adapter

IsOS calls the configured `aether-profile setup --json` or `egia-config setup --json` executable on demand. It sends one bounded JSON object on standard input: `{schema_version:1,op:"catalog"|"validate"|"render",core:"eos"|"egia",revision,graph,endpoints,packages}`. `revision` is the SHA-256 of the saved graph bytes for Run. `graph` is the full v1 graph, `endpoints` contains resolved IsOS and core listener addresses, and `packages` contains verified exact package references `{id,version,path}`. The adapter returns `{schema_version:1,ok,diagnostics:[{path,code,message}],files:[{path,content}],catalog?}`. A `render` file path must be relative and begin with its core name, such as `eos/graph.json`. Each file contains UTF-8 text. The request is at most 1 MiB, the reply at most 4 MiB, each file at most 1 MiB, and setup times out after 10 seconds. The adapter binds no listener.

An enabled EOS process node supplies `--socket` in `settings.args`; enabled EOS bridge and config nodes supply `--listen` as `127.0.0.1:<port>`. It rejects missing, malformed, or duplicate required flags before calling a core; it does not silently substitute an EOS endpoint. The reserved EGIA northbound endpoint is currently `127.0.0.1:16060`; EGIA's managed launch configuration owns its actual listener, and no EGIA process node is enabled by the starter graph. Core setup must support schema 1. An unavailable setup operation is an incompatibility and cannot fall back to reading the authored graph directly.

Apply renders enabled cores after validating the saved revision; Run and Restart recheck the revision and package inputs before starting children. IsOS checks the saved graph revision again and writes accepted files atomically under its private generated configuration directory. A stale revision aborts setup without replacing the graph. EOS's rendered `graph.json` is a validated snapshot; the current EOS runtime still reads the saved graph directly. Missing or incompatible setup adapters block launch. MuJoCo is an external IsOS package and is never built by core setup.

## Owner-only manager operations

The Unix socket accepts one JSON request per line: `{schema_version:1,op,deployment?,revision?,service_id?,cursor?,limit?}`. It replies `{schema_version:1,ok,data,error}`. The manager checks peer UID before processing. `manager_identity` returns `pid` (process ID), `start_ticks` (Linux process start identity), `mode` (`dev` or `product`), `executable` (absolute path), `executable_sha256` (digest), IsOS/API/service-contract versions, `api_origin`, and `ui_url`. Use this identity to distinguish an existing IsOS owner from a foreign port listener; a port number alone never confers ownership.

`manager_shutdown` stops all graph children and independent services, then ends the API, UI, and manager. `deployment_stop` affects only the requested deployment. `services_list`, `services_validate`, `services_status`, `services_start`, `services_stop`, `services_restart`, and `services_logs` operate on the [independent service definitions](services.md); start/stop/restart/logs require `service_id`, and logs accepts opaque `cursor` plus `limit` 1–200. A CLI with a different executable digest may still request shutdown but cannot start or restart deployment or services. `isos status --json` combines deployment status, manager identity, and service statuses; `isos version check` reports configured core, graph, and running readiness.

Log `source` may be `all`, `isos`, `services`, a service source such as `service-gateway`, `packages`, `graph`, `ui`, `eos`, `egia`, `mujoco`, `bridge`, `config`, or a validated child ID. `limit` is 1–200 (default 100). Omitting `source` uses `[logging].default_source` (`all` by default). Each line has an RFC 3339 UTC `timestamp`, `level`, `source`, `message`, `event_code`, and `pid`; internal package events also carry nullable `run_id`, `correlation_id`, and bounded `fields`. Successful HTTP install/link events record `fields.package_id`, `package_version`, `source_kind`, and available receipt `source_commit`/`asset_sha256`; a link without a release manifest uses `package_linked` rather than `package_verified`. A failure records the exact requested identity without a raw error or local path. Current MuJoCo backend and loadable-adapter resolver failures emit `package_load_failed` with the exact pin and `backend` or `adapter` stage, a manager-session `run_id`, and per-failure `correlation_id`; graph-sidecar admission and unrelated binary failures retain general diagnostics. These IDs are for log correlation, not an active graph revision. IsOS records its own diagnostics and selected structured child events; plain child stdout and stderr are not mirrored. `next_cursor` is a string even when no more lines are available; poll with it for later writes. Files have a `0700` directory and `0600` regular-file permissions, rotate after `[logging].max_bytes` bytes (default 256 KiB), and retain `[logging].backups` copies (default 2). `[logging].max_sources` caps source files (default 32); when full, new sources still write to the unified stream and remain filterable there.

`[logging].level` records file entries at or above `trace < debug < info < warn < error` (default `info`). The HTTP `level` parameter and UI selector filter displayed entries without changing recording. Terminal status and command errors are unaffected. `isos logs [source] --level warn` reads the same store.

External cores can submit selected events over the owner-only manager socket with `{schema_version:1,op:"log_append",deployment?,log:{source,level,message,event_code?}}`; the reply contains `{accepted:true}`. `source` is a validated child ID (1–48 ASCII letters, digits, `_`, or `-`) and cannot be `all`, `events`, `services`, `isos`, or `browser`; `level` is one of the five severities; `message` is 1–8192 UTF-8 bytes; optional `event_code` is 1–64 lowercase ASCII letters, digits, or underscores. The core remains responsible for its own complete logs.

When an applied graph has a MuJoCo package, IsOS passes the deployment log directory to EOS runtime. EOS alone writes and rotates `mujoco.log` using the same JSON line and permission contract; IsOS reads it through this API.

### Local runtime configuration

IsOS reads [config/isos.toml](../../../config/isos.toml) from its source checkout by default, independent of the command working directory. Set `ISOS_CONFIG` to an absolute path to require another file. Environment variables override matching TOML fields. Relative TOML source paths are resolved against the IsOS checkout; private deployment state still follows `ISOS_HOME` or XDG directories. The command working directory is not searched for `.env`. See [config/isos.example.toml](../../../config/isos.example.toml).

| TOML field | Type / unit | Purpose and validation | Environment override |
| --- | --- | --- | --- |
| `server.http_port` | integer TCP port | IsOS loopback API; 1024–65535 | `ISOS_HTTP_PORT` |
| `server.ui_port` | integer TCP port | IsosUI loopback frontend; 1024–65535, different from HTTP port | `ISOSUI_PORT` |
| `paths.ui_repo` | path string | Optional IsosUI source repository for explicit `--ui-live`; relative paths resolve from the IsOS checkout | `ISOSUI_REPO_PATH` |
| `paths.ui_dist` | path string | Prebuilt IsosUI `dist/local` bundle with manifest; relative to IsOS checkout | `ISOSUI_DIST_DIR` |
| `paths.aether_bin_dir` | path string | One prebuilt Aether Rust executable directory; relative to IsOS checkout | `ISOS_AETHER_BIN_DIR` |
| `paths.egia_bin_dir` | path string | One EGIA launcher directory containing `egia-config` and `egia`; relative to IsOS checkout | `ISOS_EGIA_BIN_DIR` |
| `paths.mujoco_root` | path string | Optional MuJoCo runtime root; relative to IsOS checkout | `MUJOCO_ROOT` |
| `deployment.name` | slug string | Default graph/log namespace; validated deployment name | `ISOS_DEPLOYMENT` |
| `logging.max_bytes` | integer bytes | Rotation threshold, 65,536–100,000,000; default 262,144 | `ISOS_LOG_MAX_BYTES` |
| `logging.backups` | integer count | Rotated copies, 1–10; default 2 | `ISOS_LOG_BACKUPS` |
| `logging.default_source` | source string | Default CLI and HTTP log source; `all` selects the unified stream; default `all` | `ISOS_LOG_DEFAULT_SOURCE` |
| `logging.max_sources` | integer count | Source file cap, 10–256; default 32; excess sources still write to the unified stream | `ISOS_LOG_MAX_SOURCES` |
| `logging.level` | enum string | Minimum file severity: `trace`, `debug`, `info`, `warn`, or `error`; default `info` | `ISOS_LOG_LEVEL` |

## Configured builds and version readiness

Build EOS, EGIA, and IsosUI in their owning repositories, then set `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist`. IsOS resolves only the configured artifacts. `isos version check` prints a readable readiness report; `isos version check --json` returns the same status for scripts. It reports each core's executable path, reported product version, setup compatibility, graph configuration, running state, and a specific action when blocked. It also identifies the IsOS executable and UI bundle. Product versions come from the cores; graph revisions and backend package versions are separate identities.

IsOS probes a core with `{schema_version:1,op:"identity",core:"eos"|"egia"}` on `aether-profile setup --json` or `egia-config setup --json`. A successful response contains `{schema_version:1,ok:true,diagnostics:[],files:[],identity:{core,product_version,graph_version:"1.0.0",node_type_version:"1.0",setup_schema_version:1}}`. This probe requires no graph or endpoint and starts no runtime. Run and Restart reject missing or incompatible binaries before launching hardware, recheck the saved revision and package inputs, and regenerate enabled core configuration when those inputs change.

Build EOS, EGIA, and IsosUI in their owning repositories and configure their paths in IsOS. After rebuilding a configured core, run `isos init` (or `isos config init`) to refresh the applied graph's launch inputs, then Run or explicitly Restart. A running process continues using its existing executable until restarted. An unapplied graph still needs Save graph before Run; `init` does not silently apply it. IsOS does not infer selection from a Git commit.

`adapter_families` entries contain `id` (stable adapter ID), `device_kinds` (device classes), `package` (optional exact `{id,version}`), `compiled` and `available` (booleans), `transport` and `description` (strings), optional `supported_primitives`, and `reason` when unavailable. `primitive_bindings` entries contain `node_type`, `device_kind`, `movement_types`, `available`, `reason`, and `primitives` with exact name, version, movement type, and description. `targets` entries contain the four-segment `path`, `node_type`, `device_kind`, `adapter_type`, `movement_types`, `available`, and `reason`. `GET /node-types` also forwards EOS `primitive_specs` and typed `data_types`.

## Release target: setting options — not implemented

The existing `GET` and `POST /api/isos/v1/node-options` envelopes remain. Add `setting_options` to `data` without removing `adapter_families`, `primitive_bindings`, or `targets`. `GET` resolves the saved graph; `POST` resolves the supplied draft without saving. IsOS derives descriptors from the registered type's `Setting.selector` and resolves package options only from an exact verified `{id,version}` receipt. It returns IDs and availability, never executable or asset paths. Recompute after a package pin, node type, or dependency setting changes. No selector response is executable authority; Apply and Run verify the package again.

| Field | JSON type / unit | Purpose and owner | Default and validation | Compatibility |
| --- | --- | --- | --- | --- |
| `setting_options` | array / — | IsOS option groups, one per registered selector in the draft | Empty array when no selectors; sorted by node path and setting | Additive response field |
| `setting_options[].node` | string / canonical four-part path | Identifies instance | Required existing node | Stable path; no opaque index |
| `setting_options[].setting` | string / setting key | Identifies registered setting | Required in its node type | Stable key |
| `setting_options[].selected` | string or null / stable option ID | Echoes draft value for a single-value selector | Null if unset; invalid current value is still echoed | Never silently replace selection |
| `setting_options[].options` | array / — | Candidate choices | Empty when provider has no choices; IDs unique within group | New options are additive |
| `options[].value` | string / stable ID | Value stored in graph settings | Required nonblank; for package assets the ID is descriptor-scoped | Must not be an absolute path |
| `options[].label` | string / — | Human display name | Required nonblank | Can change without graph migration |
| `options[].description` | string / — | Explains resource and effect | Required nonblank | Can improve without changing ID |
| `options[].available` | boolean / — | Whether this draft may Apply with that value | Required; false when receipt, asset, codec, or target is unavailable | Never inferred from label |
| `options[].reason` | string or null / — | Operator action when unavailable | Required nonblank if unavailable, null otherwise | Additive diagnostics text |

Example successful `POST` response body (target):

```json
{
  "schema_version": 1,
  "ok": true,
  "data": {
    "adapter_families": [],
    "primitive_bindings": [],
    "targets": [],
    "setting_options": [{
      "node": "eos/process/mujoco/sim",
      "setting": "scene",
      "selected": "warehouse",
      "options": [{"value": "warehouse", "label": "Warehouse", "description": "Warehouse simulation scene", "available": true, "reason": null}]
    }]
  },
  "error": null
}
```

If `selected` is missing from the verified catalog, append one unavailable option with that value, a readable label, and a reason; the editor keeps it visible and Apply rejects it. A malformed request retains the schema-1 error envelope and returns HTTP 400. A valid draft with an unknown type, invalid selector descriptor, missing package pin, or incompatible asset returns HTTP 422 and field-specific diagnostics through graph validation; an unavailable option response itself remains HTTP 200 so the editor can repair the draft. A stale revision on `PUT /node-graph` returns HTTP 409 without changing saved bytes. For example:

```json
{"schema_version":1,"ok":false,"data":null,"error":"selected scene warehouse is unavailable for eos/process/mujoco/sim.settings.scene"}
```

## Supported-version identity

The setup adapter's `identity` keeps `core`, `product_version`, and `setup_schema_version` and adds `supported_graph_versions` and `supported_node_type_versions` arrays. These are accepted input versions, not a single preferred writer version. Existing `graph_version` and `node_type_version` stay as deprecated scalar aliases for older IsOS readers. IsOS compares the saved graph's actual version, every registered type ID, exact package pins, API schema, and core executable capabilities before Apply or Run. A version mismatch names the unsupported value and owner; it never guesses an upgrade or invokes a different binary. `isos version check --json` exposes the same reasons in a script-readable report. The new writer emits graph `0.1.0` and new types `@v0.1`; the reader advertises legacy `1.0.0` and supported `@v1.*` explicitly. See [configuration and log contract](configuration.md).

### G03 migration routes

The owner-only loopback API uses the existing schema-1 `{schema_version,ok,data,error}` envelope and Host/Origin checks. A migration request operates on the saved deployment graph selected by IsOS; it never includes replacement graph bytes. `revision` and `proposed_revision` are lowercase 64-character SHA-256 hex strings of original saved bytes and canonical proposed bytes, respectively. Both are required on commit; `revision` is required on preview. The client obtains the source revision from `GET /node-graph`. `changes` is an ordered array of `{path,from,to}` entries for `/version` and mapped `/node_type` fields, at most 256 entries; all paths are JSON Pointers. More than 256 entries fails with `migration_change_limit` at `/changes`, without truncation. The `graph` response is the proposed JSON value, not proof of readiness. `can_apply` is true only after local type, edge, package, and enabled-core identity/validate/render preflight succeeds.

Preview request:

```json
{"schema_version":1,"revision":"7a8daee6b7db9105f5483506ffc1743625b63089710dfbf936622329c471016b"}
```

A well-formed preview has HTTP 200 and `data` shaped as; this example uses an older EOS setup adapter, so publication remains blocked:

```json
{"revision":"7a8daee6b7db9105f5483506ffc1743625b63089710dfbf936622329c471016b","proposed_revision":"825d667d31c455844be7f22b565e0ab6b14b44aeb7ce4861a704ffd028081df1","graph":{"version":"0.1.0","robot":{"id":"demo-0","name":"Aether deployment"},"cores":{"eos":{"layers":{}}}},"changes":[{"path":"/version","from":"1.0.0","to":"0.1.0"}],"can_apply":false,"diagnostics":[{"path":"/cores/eos","code":"owner_reader_unavailable","message":"eos setup does not advertise graph 0.1.0"}]}
```

The shortened `graph` above shows response shape; the actual response contains the **full** proposed graph, with every accepted field preserved. A preflight blocker makes `can_apply:false` with diagnostics and no saved write. Malformed input or an invalid legacy graph returns HTTP 422 with `data.diagnostics`; a stale original revision returns HTTP 409 and `error:"node graph revision changed; reload before migration"`. Unknown graph versions use `/version` and `unsupported_graph_version`; unsafe process nodes use `unsafe_process_node`; unmapped types use `unmapped_node_type` at their exact `/node_type` path. No preview result authorizes Run.

Commit request:

```json
{"schema_version":1,"revision":"7a8daee6b7db9105f5483506ffc1743625b63089710dfbf936622329c471016b","proposed_revision":"825d667d31c455844be7f22b565e0ab6b14b44aeb7ce4861a704ffd028081df1"}
```

On success, HTTP 200 returns `data:{revision:<new digest>,graph:<full saved 0.1.0 value>,active_revision:<old running digest or null>}`. A changed source or proposal returns HTTP 409; failed owner/package/edge/render preflight returns HTTP 422 with exact diagnostics. A filesystem publication failure **before graph rename** returns HTTP 500 and keeps original saved bytes. A graph-parent fsync failure after rename returns HTTP 500 with `commit_outcome_uncertain`; old bytes cannot be assumed. The caller may GET again to resolve that or a transport interruption after commit. `GET /node-graph` then reports the new saved revision and `read_only:false` when the new graph is admissible; `GET /process-status` retains the prior `active_revision` until explicit Restart.

The CLI shares the same migration engine: `isos config migrate --preview --revision <saved-sha256>` prints the proposed revision, changes, and diagnostics without writing; `isos config migrate --revision <saved-sha256> --proposed-revision <preview-sha256>` commits. It does not accept a graph file or infer an expected revision from current disk. The implementation must keep HTTP and CLI failures equivalent.

With the G03 writer, GET on a missing graph and Init produce a `0.1.0` draft; first PUT Apply uses an explicit `revision:""`, and later `0.1.0` edits use the saved revision. CLI Apply for a new or changed `0.1.0` graph uses explicit `--revision`. Init never rewrites an old `1.0.0` graph, and structurally identical legacy Apply stays a no-op. Run/Restart for the new version select the digest-checked manifest and EOS projection at `data/generated/<deployment>/<saved-revision>/` and compare every stored generated file against a fresh owner render of the exact saved graph before launch. The legacy flat `generated/<deployment>/core-setup/launch-inputs.json` is used only for `1.0.0` compatibility. See the [graph contract](node-graph.md#g03-writer-and-snapshot-selection).
