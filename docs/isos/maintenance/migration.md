# Migration from the old component runtime

[中文](../../../docs_cn/isos/maintenance/migration_cn.md)

IsOS runs only the [saved node graph](../interfaces/node-graph.md) at `deployments/<name>.nodes.json`. Old `<name>.json` component graphs, build receipts, and pinned versions are not imported or executed. IsOS leaves those files untouched and reports the required current graph path. `isos config migrate` applies only to a safe saved `1.0.0` `.nodes.json` graph.

1. Back up the old graph, stack, and core configuration. If `isos-server.service` is active, stop it with `systemctl --user stop isos-server.service`.
2. Move an old `<name>.json` or a `.nodes.json` graph with an unsupported version such as `2.0.0` to a backup path before `isos init`, or author a new `0.1.0` graph. `isos init` creates a `0.1.0` starter only when the destination is absent. Review every node, package reference, and connection; old component graphs are never converted automatically.
3. Build EOS, EGIA, and IsosUI in their repositories. Configure `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist`, then run `isos version check`.
4. Install or link exact package versions. Preview node sets into the current graph, validate it, and apply the saved revision. Obsolete MuJoCo and executable-command node shapes are rejected with field-specific guidance. After later core rebuilds, `isos init` refreshes the verified launch inputs of this applied v1 graph without changing its contents.
5. Run `isos serve`, then `isos run` in another terminal. Check `isos status --json` and unified logs. `isos deployment stop` stops the graph; `isos stop` shuts down the manager and its children.

The optional current `isos.service` runs `isos serve`. IsOS does not move model weights or rewrite old configuration.

## Saved-graph `1.0.0` → `0.1.0` migration

The explicit operation accepts a safe **saved `.nodes.json`** graph and does not import old `<name>.json` component files. First read `GET /api/isos/v1/node-graph` and record its saved-byte revision. Run `isos config migrate --preview --revision <saved-sha256>` and inspect the full proposed graph, exact type substitutions, unchanged package pins, and diagnostics. A blocked preview cannot be committed. After owner readers and package receipts are compatible, run `isos config migrate --revision <saved-sha256> --proposed-revision <preview-sha256>`. If either revision changed, reload and preview again. The original bytes remain untouched until the final atomic commit; no migration starts or restarts a process. Use `isos status --json` to see the new saved revision and the old active revision, then explicitly Restart after reviewing readiness. See the [graph migration contract](../interfaces/node-graph.md#g03-explicit-graph-migration) and [API](../interfaces/local-api.md#g03-migration-routes).

## Check connections before Apply

For a new or migrated `0.1.0` graph, run `isos config validate <draft.nodes.json>` before `isos config apply <draft.nodes.json> --revision <saved-sha256>`; use an empty revision for the first Apply. The workbench Validate API returns `edges` only when the complete graph passes. Inspect each edge's actual input-source pointer, route class, fixed flag, and QoS eligibility. Only the exact EOS camera `frame` → camera gate `frame` route accepts optional `priority` (integer `-20..19`, default `0`) or `frequency_hz` (finite positive Hz, absent means uncapped). A command, health, status, result, or other unrouted edge rejects either setting. A `routed:true` declaration is not live health. Invalid edges show their exact pointer and stable code in Validate, Apply, CLI, and migration preview; Run and Restart repeat admission before spawning children. Correct the draft or restore a valid saved revision, then Validate again. See the [edge contract](../interfaces/node-graph.md#g06-graph-wide-edge-admission--implemented-contract).

In M1, EGIA common-node edges can be inspected as `egia_local` declarations, but an EGIA-inclusive Apply requires the EGIA owner setup's real `eos_gateway` endpoint. That gateway is supplied in the later reader-feature and gRPC work; leave EGIA out of an active graph until it is configured and verified. IsOS will reject the incomplete owner setup without writing the graph.
