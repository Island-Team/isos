# Migration from legacy launchers

[中文](../../../docs_cn/isos/maintenance/migration_cn.md)

The legacy EOS and EGIA launchers and IsOS component graph remain compatibility paths. Current IsOS deployments use the [v2 node graph](../interfaces/node-graph-v2.md), exact build receipts, and the [independent service contract](../interfaces/services.md). There is no automatic TOML or graph v1 import and no `isos config migrate` command.

1. Back up the old stack, EOS, EGIA, model-service, and `<name>.json` files.
2. Run `isos init --mode dev` or `isos init --mode product`. Existing v2 authored graphs are preserved.
3. Build and select the exact IsOS, EOS, and IsosUI artifacts. Use `isos version check` to inspect selected and active identities. An IsOS executable activation requires the manager to be stopped.
4. Install or link exact package versions. Preview their node sets into a v2 draft; validate and apply the graph. Keep model-like services in per-service TOML outside this graph.
5. Stop old launchers. Start `isos serve`, then `isos run` in another terminal. Check `isos status --json` and logs. `isos deployment stop` stops only the graph; `isos stop` shuts down the manager and all owned children.

Do not let legacy and v2 managers own the same port or process. IsOS does not move model weights or rewrite old configuration automatically.
