# Migration from the old component runtime

[中文](../../../docs_cn/isos/maintenance/migration_cn.md)

IsOS runs only the [saved node graph](../interfaces/node-graph.md) at `deployments/<name>.nodes.json`. Old `<name>.json` component graphs, build receipts, and pinned versions are not imported or executed. IsOS leaves those files untouched and reports the required current graph path. There is no `isos config migrate` command.

1. Back up the old graph, stack, and core configuration. If `isos-server.service` is active, stop it with `systemctl --user stop isos-server.service`.
2. Move an old `<name>.json` or a `.nodes.json` graph with version `2.0.0` to a backup path before `isos init`, or author a new version `1.0.0` graph directly. `isos init` creates a v1 starter only when the destination is absent. Review every node, package reference, and connection; no automatic graph conversion occurs.
3. Build EOS, EGIA, and IsosUI in their repositories. Configure `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist`, then run `isos version check`.
4. Install or link exact package versions. Preview node sets into the current graph, validate it, and apply the saved revision. Obsolete MuJoCo and executable-command node shapes are rejected with field-specific guidance. After later core rebuilds, `isos init` refreshes the verified launch inputs of this applied v1 graph without changing its contents.
5. Run `isos serve`, then `isos run` in another terminal. Check `isos status --json` and unified logs. `isos deployment stop` stops the graph; `isos stop` shuts down the manager and its children.

The optional current `isos.service` runs `isos serve`. IsOS does not move model weights or rewrite old configuration.

## Planned saved-graph `1.0.0` → `0.1.0` migration

The steps above describe the current installed command set. G03 adds a separate, explicit operation for a safe **saved `.nodes.json`** graph; it does not import old `<name>.json` component files. First read `GET /api/isos/v1/node-graph` or `isos config show` and record the saved-byte revision. Run `isos config migrate --preview --revision <saved-sha256>` and inspect the full proposed graph, exact type substitutions, unchanged package pins, and diagnostics. A blocked preview cannot be committed. After owner readers and package receipts are compatible, run `isos config migrate --revision <saved-sha256> --proposed-revision <preview-sha256>`. If either revision changed, reload and preview again. The original bytes remain untouched until the final atomic commit; no migration starts or restarts a process. Use `isos status --json` to see the new saved revision and the old active revision, then explicitly Restart after reviewing readiness. See the [graph migration contract](../interfaces/node-graph.md#g03-explicit-graph-migration--target-design-not-implemented) and [API](../interfaces/local-api.md#g03-migration-routes--target-design-not-implemented). These commands are target design until G03 implementation passes its owner-reader gates.
