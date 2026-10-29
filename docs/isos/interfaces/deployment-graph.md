# Deployment graph v2

[中文](../../../docs_cn/isos/interfaces/deployment-graph_cn.md)

IsOS stores a deployment at `~/.config/isos/deployments/<name>.json`. `schema_version: 2` names exact EOS and EGIA profile versions. `layout` is presentation data. Edges point from consumer to provider; providers start first. A `depends_on` edge with `input` and `output` binds typed ports. IsOS checks IDs, cycles, profile versions, typed contracts, duplicate bindings and listeners, package receipts, and safe UDS paths before applying or running.

```json
{"schema_version":2,"name":"default","nodes":[
  {"id":"eos","kind":"component","role":"eos","enabled":false,"profile":{"id":"eos.native","version":"1.0.0"},"settings":{"north_http_url":"http://127.0.0.1:16201"}},
  {"id":"eos-uds","kind":"endpoint","owned":false,"enabled":false,"profile":{"id":"eos.uds","version":"1.0.0"},"settings":{"socket_path":"/run/user/1000/isos/default-eos.sock"}},
  {"id":"egia","kind":"component","role":"egia","enabled":false,"profile":{"id":"egia.agent","version":"1.0.0"}}
],"edges":[
  {"from":"eos-uds","to":"eos","kind":"depends_on"},
  {"from":"egia","to":"eos-uds","kind":"depends_on","input":"eos","output":"grpc"}
]}
```

The socket path above is an example; `isos init` writes the actual private runtime path. `eos.native` is the process and `eos.uds` is a logical endpoint on that same process. EGIA connects directly to EOS over that socket; IsOS only supervises. `isos version pin` adds IsOS-owned launch commands and exact build receipts. Enable the three nodes only after pinning compatible builds. EOS's HTTP sidecar is a separate optional service on port `16201`.

`isos config validate <file>` checks the graph and component-owned descriptors. `isos config apply <file>` renders EOS and EGIA config under IsOS's XDG data directory and saves the graph with no runtime startup. `isos config migrate` backs up v1 JSON and refuses unmappable fields. Running deployments retain their active graph snapshot until explicit restart.

Component `settings` use the fields exposed by `GET /api/isos/v2/profiles`; nested EGIA settings retain their object shape. Unknown settings fail validation instead of being omitted during rendering. EOS `grpc_uds_path` and EGIA `gateway` are derived from the `eos.uds` endpoint's `socket_path`. Listener settings and `launch.ready_tcp` must agree. EOS `north_http_url` and an optional North HTTP sidecar's readiness must agree with `host:north_http_port`.
