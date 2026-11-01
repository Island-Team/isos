# First use: IsOS and MuJoCo

[中文](../../../docs_cn/isos/usage/get-started_cn.md)

## Follow the running system

**Works today:** New graphs use `0.1.0` and registered `@v0.1` types. Known safe `1.0.0` graphs remain readable and require explicit migration before editing after Apply. Setting selectors, shared event fields, and EGIA integration are [later milestones](../maintenance/roadmap.md). Read status first, then logs for the same step; a startup line records a transition, while status reports the live state.

1. Install IsOS and run plain `isos`. It creates missing development configuration and graph, starts the API on 16000 and IsosUI on 16001, and opens the browser. EOS, EGIA, and MuJoCo remain stopped. Running `isos` again reuses the workbench. `isos init` performs only the file initialization and exits.

```sh
isos
```

2. Build EOS, EGIA, and IsosUI in their owning repositories; set `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist` in the IsOS checkout's `config/isos.toml`. Run `isos version check` (`--json` for scripts). Fix each reported missing executable or setup mismatch before trying to start hardware. EGIA may still report blocked while the Rust EOS gateway is unfinished. `isos serve` starts the workbench without opening a browser; `isos open` starts it if needed. `isos serve --ui-live` is for frontend development only.

For a new `0.1.0` draft, Graph shows the saved revision. Validate, then Apply with that revision. A first API PUT on a missing graph uses an explicit empty revision (`"revision":""`); after `isos init` has saved the starter, use its actual SHA-256. CLI Apply takes `--revision <saved-sha256>` for the saved starter or later edits, and `--revision ""` only when the destination file is absent. An unavailable owner reader blocks Apply and leaves the draft bytes unchanged.

3. Build the MuJoCo simulator and stage a `0.1.1` bundle using its repository's `tools/build-backend-bundle.py`; set `MUJOCO_ROOT` to the matching runtime. Link the bundle, then preview `sim-hardware` in Graph. The starter has EOS process and dispatcher nodes, but no hardware package. Preview adds the MuJoCo process, camera, base, manipulator, gate, and Vision nodes. Choose an available scene, review the draft, Validate, then Apply in Graph. Installing or linking alone never changes the graph. These CLI commands link and list the package; [package operations](packages.md) cover a CLI preview:

```sh
isos pkg link island/mujoco-sim 0.1.1 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
```

4. After Apply, run `isos run`; it stays attached. In another terminal, read `isos status --json` for the saved and active revisions, process readiness and last error. Then read `isos logs all --level info` for the startup sequence. The current IsOS start line marks a boot attempt; status confirms readiness. On a MuJoCo or loadable-adapter package load failure, read `isos logs packages --level warn` first: its `package_id`/`package_version` identify the pinned release, while `run_id` and `correlation_id` group a manager preflight failure; then inspect the command error for the specific repair. EOS and MuJoCo sources explain child startup. Use `isos logs eos --level warn` or the IsosUI Logs panel to investigate a specific failure. Log files are under the private data root's `logs/<deployment>/`; [configuration and log rules](../interfaces/configuration.md) explain paths, filtering, rotation and cursors. The target shared event adds a single ready event and per-node fields; most current events may omit them.

5. Ctrl+C stops the attached deployment. `isos deployment stop` stops only the graph; `isos stop` also stops independent services, UI, API, and manager. Repeated stops succeed. After stopping, confirm `isos status --json` says stopped, then read warning/error logs if the stop was abnormal. A clean stop does not need a flood of per-tick messages.

Run rechecks the exact graph revision, configured executables, package, scene, and assets. EOS starts and owns its MuJoCo child. Installing a package, changing a configured build path, or editing a draft does not alter an active deployment; Apply and Restart explicitly. See [package commands](packages.md), [graph fields](../interfaces/node-graph.md), and the [independent services contract](../interfaces/services.md).

For an applied `0.1.0` graph, Run compares the revisioned generated files with a fresh owner render before launch. If a configured EOS or EGIA binary changes, `isos init` reports the stale launch input and preserves the immutable snapshot. Restore the pinned binary or create a new graph revision and Apply before running again.

If new children fail during a restart, IsOS stops that attempt and restarts the previous process specifications under the previous active revision. If the previous children cannot be restored, status reports a degraded deployment and the restart error.

### Migrate a saved legacy graph

For an applied `1.0.0` graph, read its saved revision from Graph or `GET /api/isos/v1/node-graph` before editing. Preview reports exact type substitutions, the full `0.1.0` proposal, and blockers without changing saved bytes or running children:

```sh
isos config migrate --preview --revision <saved-sha256>
```

Review the proposal and resolve owner-reader or package blockers. Then commit **the previewed digest** with both revisions:

```sh
isos config migrate --revision <saved-sha256> --proposed-revision <preview-sha256>
```

A stale source or proposal digest requires a new preview. `isos status --json` then shows the new saved revision and prior active revision until explicit Restart. Unmatched legacy types stay read-only; use the [migration guide](../maintenance/migration.md#planned-saved-graph-100--010-migration) to repair them explicitly.
