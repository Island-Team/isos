# Local manager API

The IsOS command line calls the manager core directly. Other local clients can use the owner-only Unix socket. The former loopback HTTP/WebSocket bridge has been removed.

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

## Authentication and transport

Run `isos daemon` to listen on `$XDG_RUNTIME_DIR/isos/manager.sock`. The parent directory has mode `0700` and the socket has mode `0600`. Each connection sends one JSON request line and receives one JSON response line.

Requests use `schema_version: 1` and an `op` of `status`, `graph`, `start`, `stop`, or `apply`. The deployment defaults to `ISOS_DEPLOYMENT` or `default`; component operations may set `role` to `eos` or `egia`.

    {"schema_version":1,"op":"status","deployment":"default","role":"eos"}

Successful operations return the versioned envelope:

    {"schema_version":1,"ok":true,"data":{"started":["eos"]},"error":null}

Validation failures return the same envelope with `ok: false`, `data: null`, and an actionable `error` string. The CLI's `isos eos status --json` command returns unwrapped status data for compatibility with `eos-cli`.

The socket exposes deployment status, graph read/apply, and start/stop operations. Apply validates and formats the complete graph, renders activation templates and generated TOML, and saves it without restarting active nodes. Start and stop honor dependency order and the recorded active graph snapshot.

Related: [deployment graph](deployment-graph.md), [release manifest](releases.md).
