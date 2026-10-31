# Migration from legacy launchers

[中文](../../../docs_cn/isos/maintenance/migration_cn.md)

The legacy EOS and EGIA launchers and IsOS component graph remain compatibility paths. Current IsOS deployments use the [v2 node graph](../interfaces/node-graph-v2.md), configured prebuilt core binaries, and the [independent service contract](../interfaces/services.md). There is no automatic TOML or graph v1 import and no `isos config migrate` command.

1. Back up the old stack, EOS, EGIA, model-service, and `<name>.json` files.
2. Run `isos init --mode dev` or `isos init --mode product`. Existing v2 authored graphs are preserved.
3. Build EOS, EGIA, and IsosUI in their own repositories. Configure `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist`, then run `isos version check` to inspect compatibility and readiness. Stop the manager before replacing its own executable.
4. Install or link exact package versions. Preview their node sets into a v2 draft; validate and apply the graph. Keep model-like services in per-service TOML outside this graph.
5. Stop old launchers. Start `isos serve`, then `isos run` in another terminal. Check `isos status --json` and logs. `isos deployment stop` stops only the graph; `isos stop` shuts down the manager and all owned children.

Do not let legacy and v2 managers own the same port or process. IsOS does not move model weights or rewrite old configuration automatically.
