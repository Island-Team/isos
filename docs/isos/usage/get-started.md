# Get started with IsOS and MuJoCo v2

[中文](../../../docs_cn/isos/usage/get-started_cn.md)

Install IsOS, then run plain `isos`. It initializes a missing development graph, starts the IsOS API on 16000 and IsosUI on 16001, and opens the browser. Backend cores stay stopped. Running `isos` again reuses the workbench. `isos init` only creates missing configuration and graph files and then exits.

```sh
isos
```

Build EOS, EGIA, and IsosUI in their owning repositories, then set `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist` in `config/isos.toml`. `isos version check` reports whether each configured core can run; use `--json` for scripts. MuJoCo remains an external IsOS package. `isos serve` starts the workbench without opening a browser; `isos open` starts it if needed. Use `isos serve --ui-live` only when developing the frontend from an IsosUI checkout with npm dependencies installed. `isos build` and `isos version select` provide migration guidance instead of building or selecting cores.

Build the MuJoCo simulator and stage a `2.0.0` bundle with its repository's `tools/build-backend-bundle.py`; set `MUJOCO_ROOT` to the matching runtime. Link the bundle and preview its `sim-hardware` node set in the Graph workbench. The starter graph contains EOS process and dispatcher nodes but no hardware package. The preview adds the MuJoCo process, camera, base, manipulator, gate, and Vision nodes. Choose a scene, inspect the draft, then Validate and Apply. CLI equivalents are:

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`run`, `start`, and `restart` stay attached. Ctrl+C stops the deployment. In another terminal, `isos status --json` shows the manager and active revision; `isos version check` reports configured build, graph, and running readiness. `isos deployment stop` stops only the graph, while `isos stop` also stops services, UI, API, and manager. Repeated stops succeed.

Run rechecks the exact graph revision, configured executables, package, scene, and assets. EOS starts and owns its MuJoCo child. Installing a package, changing a configured build path, or editing a draft does not alter an active deployment; apply the change and restart explicitly. See [package commands](packages.md), [graph fields](../interfaces/node-graph-v2.md), and the [independent services contract](../interfaces/services.md).

If new children fail during a restart, IsOS stops that attempt and restarts the previous process specifications under the previous active revision. If the previous children cannot be restored, status reports a degraded deployment and the restart error.
