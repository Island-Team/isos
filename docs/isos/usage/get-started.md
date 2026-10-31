# Get started with IsOS and MuJoCo v2

[中文](../../../docs_cn/isos/usage/get-started_cn.md)

Install IsOS, then initialize a development home. `init` writes configuration and an editable starter graph only when absent, then exits. It does not start the manager or UI.

Run `isos` with no command to print the short first-use guide. After `isos init`, the CLI prints the next build, editor, and graph commands; these are suggestions and do not start a service.

```sh
isos init --mode dev
isos build
isos serve
```

`isos build` uses local source and activates the new EOS and IsosUI builds. MuJoCo remains an external IsOS package. `isos serve` builds a missing or stale dev UI bundle, then runs the manager, API, and verified static IsosUI in the foreground. Run `isos open` in another terminal. Use `isos serve --ui-live` only when developing the frontend from an IsosUI checkout with npm dependencies installed. Product mode requires clean, selected builds and serves the pinned UI without npm or source checkouts. See the [README](../../../README.md) for product setup.

Build the MuJoCo simulator and stage a `2.0.0` bundle with its repository's `tools/build-backend-bundle.py`; set `MUJOCO_ROOT` to the matching runtime. Link the bundle and preview its `sim-hardware` node set in the Graph workbench. The starter graph contains EOS process and dispatcher nodes but no hardware package. The preview adds the MuJoCo process, camera, base, manipulator, gate, and Vision nodes. Choose a scene, inspect the draft, then Validate and Apply. CLI equivalents are:

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`run`, `start`, and `restart` stay attached. Ctrl+C stops the deployment. In another terminal, `isos status --json` shows the manager and active revision; `isos version check` compares configured, selected, and active identities. `isos deployment stop` stops only the graph, while `isos stop` also stops services, UI, API, and manager. Repeated stops succeed.

Run rechecks the exact graph revision, executable receipts, package, scene, and assets. EOS starts and owns its MuJoCo child. Installing a package, staging a build, or editing a draft does not alter an active deployment; select or apply the change and restart explicitly. See [package commands](packages.md), [graph fields](../interfaces/node-graph-v2.md), and the [independent services contract](../interfaces/services.md).

If new children fail during a restart, IsOS stops that attempt and restarts the previous process specifications under the previous active revision. If the previous children cannot be restored, status reports a degraded deployment and the restart error.
