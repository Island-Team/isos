# First use: IsOS and MuJoCo

[中文](../../../docs_cn/isos/usage/get-started_cn.md)

## Follow the running system

**Works today:** The commands below use the current `1.0.0` graph reader and installed `@v1.*` types. The proposed `0.1.0` graph writer, setting selectors, shared event fields, and EGIA integration are [implementation milestones](../maintenance/roadmap.md). A new operator should read status first, then logs for the same step; a startup line is evidence of a transition, while status is the live authority.

1. Install IsOS and run plain `isos`. It creates missing development configuration and graph, starts the API on 16000 and IsosUI on 16001, and opens the browser. EOS, EGIA, and MuJoCo remain stopped. Running `isos` again reuses the workbench. `isos init` performs only the file initialization and exits.

```sh
isos
```

2. Build EOS, EGIA, and IsosUI in their owning repositories; set `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist` in the IsOS checkout's `config/isos.toml`. Run `isos version check` (`--json` for scripts). Fix each reported missing executable or setup mismatch before trying to start hardware. EGIA may still report blocked while the Rust EOS gateway is unfinished. `isos serve` starts the workbench without opening a browser; `isos open` starts it if needed. `isos serve --ui-live` is for frontend development only.

3. Build the MuJoCo simulator and stage a `0.1.1` bundle using its repository's `tools/build-backend-bundle.py`; set `MUJOCO_ROOT` to the matching runtime. Link the bundle, then preview `sim-hardware` in Graph. The starter has EOS process and dispatcher nodes, but no hardware package. Preview adds the MuJoCo process, camera, base, manipulator, gate, and Vision nodes. Choose an available scene, review the draft, Validate, then Apply in Graph. Installing or linking alone never changes the graph. These CLI commands link and list the package; [package operations](packages.md) cover a CLI preview:

```sh
isos pkg link island/mujoco-sim 0.1.1 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
```

4. After Apply, run `isos run`; it stays attached. In another terminal, read `isos status --json` for the saved and active revisions, process readiness and last error. Then read `isos logs all --level info` for the startup sequence. The current IsOS start line marks a boot attempt; status confirms readiness. On a MuJoCo or loadable-adapter package load failure, read `isos logs packages --level warn` first: its `package_id`/`package_version` identify the pinned release, while `run_id` and `correlation_id` group a manager preflight failure; then inspect the command error for the specific repair. EOS and MuJoCo sources explain child startup. Use `isos logs eos --level warn` or the IsosUI Logs panel to investigate a specific failure. Log files are under the private data root's `logs/<deployment>/`; [configuration and log rules](../interfaces/configuration.md) explain paths, filtering, rotation and cursors. The target shared event adds a single ready event and per-node fields; most current events may omit them.

5. Ctrl+C stops the attached deployment. `isos deployment stop` stops only the graph; `isos stop` also stops independent services, UI, API, and manager. Repeated stops succeed. After stopping, confirm `isos status --json` says stopped, then read warning/error logs if the stop was abnormal. A clean stop does not need a flood of per-tick messages.

Run rechecks the exact graph revision, configured executables, package, scene, and assets. EOS starts and owns its MuJoCo child. Installing a package, changing a configured build path, or editing a draft does not alter an active deployment; Apply and Restart explicitly. See [package commands](packages.md), [graph fields](../interfaces/node-graph.md), and the [independent services contract](../interfaces/services.md).

If new children fail during a restart, IsOS stops that attempt and restarts the previous process specifications under the previous active revision. If the previous children cannot be restored, status reports a degraded deployment and the restart error.
