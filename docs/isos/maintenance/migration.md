# Migration from the old component runtime

[中文](../../../docs_cn/isos/maintenance/migration_cn.md)

IsOS runs only the [saved node graph](../interfaces/node-graph-v1.md) at `deployments/<name>.nodes.json`. Old `<name>.json` component graphs, build receipts, and pinned versions are not imported or executed. IsOS leaves those files untouched and reports the required current graph path. There is no `isos config migrate` command.

1. Back up the old graph, stack, and core configuration. If `isos-server.service` is active, stop it with `systemctl --user stop isos-server.service`.
2. Move an old `<name>.json` aside manually before `isos init`, or author a new `<name>.nodes.json` directly. Review every node, package reference, and connection; no automatic graph conversion occurs.
3. Build EOS, EGIA, and IsosUI in their repositories. Configure `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist`, then run `isos version check`.
4. Install or link exact package versions. Preview node sets into the current graph, validate it, and apply the saved revision. Obsolete MuJoCo and executable-command node shapes are rejected with field-specific guidance.
5. Run `isos serve`, then `isos run` in another terminal. Check `isos status --json` and unified logs. `isos deployment stop` stops the graph; `isos stop` shuts down the manager and its children.

The optional current `isos.service` runs `isos serve`. IsOS does not move model weights or rewrite old configuration.
