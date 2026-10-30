# Local IsOS v2 API

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

`isos init` starts the IsOS API on `127.0.0.1:16000` by default (`ISOS_HTTP_PORT` selects another loopback port), supervises IsosUI's `npm start` server on `127.0.0.1:16001` (`ISOSUI_PORT` selects another port), and opens an owner-only Unix manager socket below `$XDG_RUNTIME_DIR/isos/` (or `ISOS_HOME/run/`). The runtime directory is mode `0700`, and the manager socket is mode `0600`. The HTTP response envelope is `{schema_version:2,ok,data,error}`. The manager's one-line Unix envelope remains schema 1 internally.

The API binds only to `127.0.0.1` and serves no frontend files. IsosUI calls it directly by address and port without a URL or bearer token. The API `Host` must match its loopback address and configured port. Browser requests may use the npm UI origin on `127.0.0.1:16001` (or configured `ISOSUI_PORT`), the API's own origin, or the hosted IsosUI origins `https://www.os-island.com` and `https://os-island.com`; other Origins are rejected. Cross-origin preflight permits GET, POST, PUT, and Content-Type. Ctrl+C stops both servers and the owner-only manager.

| Route | Response or request | Purpose |
| --- | --- | --- |
| `GET /api/isos/v2/node-graph` | `{revision,graph}` | Read the single saved graph. A missing graph returns an editable empty v2 draft. |
| `PUT /api/isos/v2/node-graph` | `{schema_version:2,revision,graph}` | Validate and apply the graph. Stale revision returns 409. |
| `POST /api/isos/v2/node-graph/validate` | `{schema_version:2,graph}` → `{valid,diagnostics}` | Registry and package admission, without applying. |
| `GET /api/isos/v2/node-types` | `{types:[...]}` | Node type catalog from `aether-profile`. |
| `GET /api/isos/v2/node-sets` | `{sets:[{package,id,parameters,nodes}]}` | Installed package node sets. |
| `POST /api/isos/v2/node-sets/preview` | `{package:{id,version},set_id,graph,values:{name:string}}` → `{graph,added_paths}` | Compose a set into a draft without saving it; invalid composition returns 422. |
| `GET /api/isos/v2/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | Installed/linked v2 backend choices and prerequisite checks. |
| `POST /api/isos/v2/packages/link` | `{id,version,path,manifest?}` | Link a local bundle; `manifest` is an optional absolute v4 release JSON path. If omitted, IsOS reads `<path>/release.json` when present. |
| `GET /api/isos/v2/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home setup summary. |
| `POST /api/isos/v2/process-control` | `{schema_version:2,operation,revision?}` | `run`, `start`, `stop`, or `restart`. The optional revision prevents acting on a stale editor view. |
| `GET /api/isos/v2/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | Pinned package and direct-child status. |
| `GET /api/isos/v2/process-logs/{node}?lines=100` | `{node,lines}` | Recent stdout/stderr lines, capped at 1000. |
| `GET /api/isos/v2/logs?source=isos&cursor=...&limit=100` | `{lines:[{timestamp,level,source,message}],next_cursor,rotated}` | Read categorized JSON logs, at most 200 lines or 64 KiB per call. `cursor` is opaque; `rotated` signals a replaced file. |
| `GET /api/isos/v2/version` | `{api_schema_version,isos_version}` | Workbench compatibility check. |

The process manager validates the saved graph and exact package again before starting children. It starts MuJoCo before EOS, and stops EOS before MuJoCo. Graph layout belongs to the editor; the HTTP graph contains runtime configuration and typed connections only.

Log `source` may be `isos`, `packages`, `graph`, `ui`, `eos`, `egia`, `mujoco`, `bridge`, `config`, or a validated child ID. `limit` is 1–200 (default 100). Each line has an RFC 3339 UTC `timestamp` string, `level` string (`info` or `error` for child streams), `source` string, and `message` string. `next_cursor` is a string even when no more lines are available; poll with it for later writes. Files have a `0700` directory and `0600` regular-file permissions, rotate after `[logging].max_bytes` bytes (default 1 MiB), and retain `[logging].backups` copies (default 3). The legacy process endpoint reads mirrored raw lines.

When an applied graph has a MuJoCo package, IsOS passes the deployment log directory to EOS runtime. EOS alone writes and rotates `mujoco.log` using the same JSON line and permission contract; IsOS reads it through this API.

### Local runtime configuration

IsOS reads `~/.config/isos/config.toml` (or `ISOS_HOME/config/config.toml`) if present. Set `ISOS_CONFIG` to an absolute path to require another file. Environment variables override matching TOML fields. The command working directory is not searched for `.env`. See [config.example.toml](../../../config.example.toml).

| TOML field | Type / unit | Purpose and validation | Environment override |
| --- | --- | --- | --- |
| `server.http_port` | integer TCP port | IsOS loopback API; 1024–65535 | `ISOS_HTTP_PORT` |
| `server.ui_port` | integer TCP port | IsosUI loopback frontend; 1024–65535, different from HTTP port | `ISOSUI_PORT` |
| `paths.ui_repo` | absolute path string | IsosUI repository with npm dependencies | `ISOSUI_REPO_PATH` |
| `paths.aether_bin_dir` | absolute path string | Aether Rust executable directory | `ISOS_AETHER_BIN_DIR` |
| `paths.mujoco_root` | absolute path string | Optional MuJoCo runtime root | `MUJOCO_ROOT` |
| `deployment.name` | slug string | Default graph/log namespace; validated deployment name | `ISOS_DEPLOYMENT` |
| `logging.max_bytes` | integer bytes | Rotation threshold, 65,536–100,000,000; default 1,048,576 | `ISOS_LOG_MAX_BYTES` |
| `logging.backups` | integer count | Rotated copies, 1–10; default 3 | `ISOS_LOG_BACKUPS` |
