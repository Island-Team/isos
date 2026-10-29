# Local IsOS API

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

`isos init` starts one IsOS user service. It serves `/api/isos/v2` on `127.0.0.1:16000` and, when pinned, a verified local IsosUI bundle. Its owner-only control socket is `$XDG_RUNTIME_DIR/isos/manager.sock` (`0700` parent, `0600` socket). EOS and EGIA expose no configuration HTTP server.

The server verifies a local bundle when it first serves it and when the pin changes. `isos open` and `isos version check` verify the bundle on each invocation, so a damaged pin fails rather than opening the hosted editor.

`isos open` opens the editor without a token in the URL. Browser requests from `http://127.0.0.1:16000`, `https://www.os-island.com`, or `https://os-island.com` may use the loopback API without a bearer token. Same-origin browser GET requests that omit `Origin` are recognized by `Sec-Fetch-Site: same-origin`. Other clients must send `Authorization: Bearer <token>`. All API requests require exact `Host: 127.0.0.1:16000`; an unapproved `Origin` gets 403, even with a bearer. CORS preflights allow GET, POST, PUT, Authorization, and Content-Type. Requests without an approved browser origin or valid bearer get 401. Legacy editors may still supply a bearer from a URL fragment; it remains in memory only.

| Route | Purpose |
| --- | --- |
| `GET /api/isos/v2/version` | Read `{api_schema_version:2,isos_version}` for editor compatibility |
| `GET /api/isos/v2/graph` | Read `{revision,graph}` |
| `POST /api/isos/v2/graph/validate` | Check `{schema_version:2,graph}` |
| `PUT /api/isos/v2/graph` | Apply `{schema_version:2,revision,graph}`; stale revision returns 409 |
| `GET /api/isos/v2/profiles` | Read EOS/EGIA node descriptors with ordered editable field metadata and nested defaults |
| `GET/POST /api/isos/v2/config/{eos|egia}/{operation}` | Call isolated component config operation |

Successful responses use `{ "schema_version":2, "ok":true, "data":..., "error":null }`. Validation diagnostics include `path`, `code`, and `message`. The private UDS control API retains its v1 one-line JSON operations (`status`, `graph`, `start`, `stop`, `apply`) for CLI clients.

Each profile in `data.profiles` keeps its `id`, `version`, `label`, `description`, `inputs`, and `outputs`. `settings_fields` is an ordered array of `{path,type,label,description?,required?,default?,min?,max?,options?,read_only?}`. Dotted paths address nested settings. `defaults` is a nested object shaped like graph `settings`; derived EOS UDS and EGIA gateway values are omitted from it. EOS `north_http_url` is derived from `host` and `north_http_port`. For example, `server.port` has a numeric field and `defaults.server.port` has its value. The adapter may expose additional field metadata; this route presents the stable editor subset.

Graph validation and Apply reject component settings that the adapter cannot use, including unknown nested keys. They validate edited values through the component adapter. EOS `launch.ready_tcp` must match `host:grpc_port`, EGIA `launch.ready_tcp` must match `server.host:server.port`, and an EOS North HTTP sidecar's readiness must match `host:north_http_port`. EOS `settings.north_http_url` must match the HTTP listener. The EOS UDS endpoint owns `settings.socket_path`; IsOS supplies that path to both component adapters. Diagnostics point to the affected node setting or launch field when possible.
