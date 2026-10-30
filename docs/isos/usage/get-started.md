# Get started with IsOS and MuJoCo v2

[中文](../../../docs_cn/isos/usage/get-started_cn.md)

Install IsOS and the Aether Rust binaries. Build the MuJoCo C++ simulator, set `MUJOCO_ROOT`, and stage a bundle with the MuJoCo repository's `tools/build-backend-bundle.py`. Link the exact bundle version, then initialize a graph:

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle
isos init
```

Leave `isos init` running in this terminal. It owns the workbench server and process manager; Ctrl+C stops both and their children. In a second terminal, run `isos open` and edit the graph.

Home shows the package, scene choices, prerequisites, saved revision, active revision, process status, and logs. Graph edits the one shared IsOS/EOS/EGIA graph. The starter graph has a backend process, EOS runtime, HTTP bridge, catalog, camera, base, manipulator, dispatcher, and a design-only EGIA guard. Select the scene and inspect executable paths, then Validate and Apply. CLI equivalents are:

```sh
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`run`, `start`, and `restart` remain attached to the terminal. Ctrl+C requests a clean stop. Use another terminal for `status`, `logs`, or `stop` while the deployment is running.

Run rechecks the exact package and graph, starts EOS, and lets EOS start MuJoCo before admitting hardware operations. Stop terminates EOS and its simulator child. Installing a package or changing an editor draft does not alter running processes; apply and restart to activate a new revision. A local link is revalidated on every apply/run. See [package commands](packages.md) and [graph fields](../interfaces/node-graph-v2.md).
