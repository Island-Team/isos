# Legacy component graph

[中文](../../../docs_cn/isos/interfaces/deployment-graph_cn.md)

This page describes the legacy `<name>.json` component graph used by the compatibility `isos eos`, `isos egia`, and `isos daemon` commands. It is not the applied v2 deployment graph. New work uses the [shared v2 node graph](node-graph-v2.md) at `<IsOS config>/deployments/<name>.nodes.json` and the [local v2 API](local-api.md).

The active v2 graph is shown with `isos config show`, validated with `isos config validate <file>`, and applied with `isos config apply <file>`. `isos version select eos <selector>` updates the three EOS executable paths through a revision-checked graph apply. Independent model-like services use [versioned TOML files](services.md), not graph service nodes. There is no `isos config migrate` command; migrate an old graph by authoring a v2 draft, validating it, and applying it explicitly. Keep a backup of the old JSON.
