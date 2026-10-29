# 部署图 v2

[English](../../../docs/isos/interfaces/deployment-graph.md)

IsOS 将部署图保存于 `~/.config/isos/deployments/<name>.json`。`schema_version: 2` 引用 EOS 与 EGIA 的精确节点配置版本。`layout` 仅用于页面展示。边从消费者指向提供者，提供者先启动。包含 `input` 和 `output` 的 `depends_on` 边绑定带类型的端口。应用或运行前，IsOS 校验 ID、环路、配置版本、协议契约、重复绑定与监听端口、软件包收据及 UDS 路径。

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

上述套接字路径仅为示例；`isos init` 会写入实际私有运行目录。`eos.native` 表示进程，`eos.uds` 是同一进程上的逻辑端点。EGIA 通过该套接字直接调用 EOS，IsOS 仅负责监督。`isos version pin` 加入由 IsOS 管理的启动命令和精确构建收据。选定兼容构建后再启用三个节点。EOS 的 HTTP 边车服务是独立可选节点，使用端口 `16201`。

`isos config validate <文件>` 校验部署图和组件提供的描述。`isos config apply <文件>` 在 IsOS XDG 数据目录渲染 EOS/EGIA 配置并保存部署图，不启动运行时。`isos config migrate` 备份 v1 JSON，拒绝无法映射的字段。运行中的部署保持启动时的配置图快照，需明确重启才能切换。

组件 `settings` 使用 `GET /api/isos/v2/profiles` 提供的字段；嵌套的 EGIA 设置保持对象结构。未知设置会在校验时失败，而不会在渲染时被忽略。EOS 的 `grpc_uds_path` 和 EGIA 的 `gateway` 由 `eos.uds` 端点的 `socket_path` 推导。监听设置必须与 `launch.ready_tcp` 一致。EOS 的 `north_http_url` 和可选 HTTP 边车服务的就绪地址必须与 `host:north_http_port` 一致。
