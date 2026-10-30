# Local IsOS v2 API

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

`isos init` starts the local workbench service on `127.0.0.1:16000` by default (`ISOS_HTTP_PORT` selects another loopback port for isolated local testing) and an owner-only Unix manager socket below `$XDG_RUNTIME_DIR/isos/` (or `ISOS_HOME/run/`). The runtime directory is mode `0700`, and the manager socket is mode `0600`. The HTTP response envelope is `{schema_version:2,ok,data,error}`. The manager's one-line Unix envelope remains schema 1 internally.

The server accepts requests from its own origin and the hosted IsosUI origins `https://www.os-island.com` and `https://os-island.com`; other clients need a bearer token. The `Host` must be `127.0.0.1:16000`. Cross-origin preflight permits GET, POST, PUT, Authorization, and Content-Type.

| Route | Response or request | Purpose |
| --- | --- | --- |
| `GET /api/isos/v2/node-graph` | `{revision,graph}` | Read the single saved graph. A missing graph returns an editable empty v2 draft. |
| `PUT /api/isos/v2/node-graph` | `{schema_version:2,revision,graph}` | Validate and apply the graph. Stale revision returns 409. |
| `POST /api/isos/v2/node-graph/validate` | `{schema_version:2,graph}` → `{valid,diagnostics}` | Registry and package admission, without applying. |
| `GET /api/isos/v2/node-types` | `{types:[...]}` | Node type catalog from `aether-profile`. |
| `GET /api/isos/v2/backends` | `{backends:[{id,version,linked,scenes,devices,ready,issues}]}` | Installed/linked v2 backend choices and prerequisite checks. |
| `GET /api/isos/v2/setup-status` | `{ready,issues,graph_revision,backends,process}` | Home setup summary. |
| `POST /api/isos/v2/process-control` | `{schema_version:2,operation,revision?}` | `run`, `start`, `stop`, or `restart`. The optional revision prevents acting on a stale editor view. |
| `GET /api/isos/v2/process-status` | `{saved_revision,active_revision,state,last_error,package,processes}` | Pinned package and direct-child status. |
| `GET /api/isos/v2/process-logs/{node}?lines=100` | `{node,lines}` | Recent stdout/stderr lines, capped at 1000. |
| `GET /api/isos/v2/version` | `{api_schema_version,isos_version}` | Workbench compatibility check. |

The process manager validates the saved graph and exact package again before starting children. It starts MuJoCo before EOS, and stops EOS before MuJoCo. Graph layout belongs to the editor; the HTTP graph contains runtime configuration and typed connections only.
