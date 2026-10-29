# Local IsOS API

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

`isos init` starts one IsOS user service. It serves `/api/isos/v2` on `127.0.0.1:16000` and, when pinned, a verified local IsosUI bundle. Its owner-only control socket is `$XDG_RUNTIME_DIR/isos/manager.sock` (`0700` parent, `0600` socket). EOS and EGIA expose no configuration HTTP server.

The server verifies a local bundle when it first serves it and when the pin changes. `isos open` and `isos version check` verify the bundle on each invocation, so a damaged pin fails rather than opening the hosted editor.

`isos open` passes a temporary bearer token in the URL fragment. The editor removes the fragment and keeps the token only in memory. HTTP API requests require `Authorization: Bearer <token>` and exact `Host: 127.0.0.1:16000`. If an `Origin` is present, it must be `http://127.0.0.1:16000`, `https://www.os-island.com`, or `https://os-island.com`. Successful CORS preflights allow GET, POST, PUT, Authorization, and Content-Type. An origin outside this list or a wrong Host gets 403; missing or invalid bearer gets 401. The bearer token is never stored in browser persistent storage.

| Route | Purpose |
| --- | --- |
| `GET /api/isos/v2/version` | Read `{api_schema_version:2,isos_version}` for editor compatibility |
| `GET /api/isos/v2/graph` | Read `{revision,graph}` |
| `POST /api/isos/v2/graph/validate` | Check `{schema_version:2,graph}` |
| `PUT /api/isos/v2/graph` | Apply `{schema_version:2,revision,graph}`; stale revision returns 409 |
| `GET /api/isos/v2/profiles` | Read flattened EOS/EGIA node descriptors |
| `GET/POST /api/isos/v2/config/{eos|egia}/{operation}` | Call isolated component config operation |

Successful responses use `{ "schema_version":2, "ok":true, "data":..., "error":null }`. Validation diagnostics include `path`, `code`, and `message`. The private UDS control API retains its v1 one-line JSON operations (`status`, `graph`, `start`, `stop`, `apply`) for CLI clients.
