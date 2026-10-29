# 部署图契约

IsOS 在 ~/.config/isos/deployments/<name>.json 保存唯一的、格式化的版本化部署图。命令行和本地管理接口使用同一契约；可选的 layout 字段不影响运行。

[English](../../../docs/isos/interfaces/deployment-graph.md)

图的 schema_version 目前为 1。nodes 是节点数组；edges 是依赖数组。节点字段包括 id、kind、role、enabled、owned、package、launch、contract 和 settings。kind 可为 component、service、backend、skill；component 的 role 为 eos 或 egia。enabled 和 owned 默认为 true。已启用且归 IsOS 管理的进程节点必须有 launch.command 数组；外部共享服务可设 owned=false，IsOS 不会关闭它。EOS 与 EGIA 同时启用时，contract 必须一致，且 EGIA 必须依赖 EOS。

边从使用者指向提供者：from 使用 to，因此 to 先启动。kind 可为 depends_on、model_binding、backend_attachment 或 skill_load。model_binding 的提供者必须为 service；backend_attachment 必须为 backend；skill_load 必须为 skill。循环、缺失节点、禁用依赖、重复边和协议不匹配都会被拒绝。

最小有效输入：

    {
      "schema_version": 1,
      "name": "default",
      "nodes": [{
        "id": "eos",
        "kind": "component",
        "role": "eos",
        "contract": "island.eos.v1",
        "launch": {"command": ["/path/to/eosd", "--config", "/path/to/eos.toml"]},
        "settings": {"north_http_url": "http://127.0.0.1:16005"}
      }],
      "edges": []
    }

使用 isos config validate <file> 校验，再以 isos config apply <file> 应用。成功后写入已选择的 JSON 和生成的 stack.toml，但不会重启现有进程。接口中的错误示例：

    {"schema_version":1,"ok":false,"data":null,"error":"EOS and EGIA require the same explicit protocol contract"}

Python 组件默认使用 Python 3.12；launch.command 可以指向 Conda、uv 或明确的解释器路径。模型服务可使用 Island-models 的 --managed --only <alias>，EGIA 进程使用 --managed --no-tui。

EOS 的 north HTTP 投影应作为 service 节点，设置 settings.sidecar_of = "eos"，并用 depends_on 边指向 EOS。这样 isos eos start 会在 eosd 就绪后启动该服务，isos eos enter 才能稳定连接。当前 EOS north HTTP 配置默认监听 127.0.0.1:16005。

相关：[本地管理 API](local-api_cn.md)、[发行清单](releases_cn.md)。
