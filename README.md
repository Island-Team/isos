# IsOS

IsOS owns the shared Aether v2 graph, backend packages, local workbench API, and direct-child process lifecycle. MuJoCo is the first backend package. EOS reads the graph, starts its verified MuJoCo child, and connects typed adapters to it. EGIA common nodes are available for graph design; they do not start an EGIA process yet.

## Start a MuJoCo deployment

Run `./install.sh` from the v2 source checkout to install the CLI without installing or starting a systemd user unit. Plain `isos` initializes a missing development graph, starts the local workbench, and opens its browser page; backend cores stay stopped. Repeating it reuses the running workbench. `isos init --mode dev` prepares the graph without launching anything, while `isos init --mode product` selects product admission rules. `isos serve` runs the workbench in the foreground without opening a browser; `isos serve --ui-live` explicitly uses supervised Webpack in dev mode.

Edit the tracked [config/isos.toml](config/isos.toml) for portable runtime defaults; [config/isos.example.toml](config/isos.example.toml) shows optional fields. Relative source paths are resolved from this IsOS checkout, regardless of the command directory. Environment variables override file values; `ISOS_CONFIG` selects another absolute config file. Private deployment graphs remain under `~/.config/isos/`. IsOS does not load `.env` from the command directory. Build EOS, EGIA, and IsosUI in their owning repositories, then configure `paths.aether_bin_dir`, `paths.egia_bin_dir`, and `paths.ui_dist` in this file; MuJoCo remains a separate IsOS package:

```sh
isos
```

`isos version check` reports configured core paths, product versions, compatibility, graph configuration, and running state; `--json` emits the report for scripts. `isos build` and `isos version select` give migration guidance instead of compiling or selecting v2 cores. `ISOSUI_REPO_PATH` or `[paths].ui_repo` is used only by explicit `isos serve --ui-live`.

`isos` and `isos serve` host the configured IsosUI bundle at `http://127.0.0.1:16001/en/isos` and the API at `http://127.0.0.1:16000/api/isos/v2`. IsOS reserves ports 16000–16019, EOS 16020–16059 (control 16020, runtime catalog 16021), and EGIA 16060–16099 (northbound 16060). Explicit listener overrides are supported; collisions are rejected before graph launch. Product mode serves the configured verified static UI and requires compatible core binaries. Ctrl+C or `isos stop` stops the UI, API, deployment, explicitly started services, and manager. `isos deployment stop` stops the graph only.

Keep the `isos serve` terminal open while using the workbench. Save and Run, Restart, and a rejected graph launch control the deployment only; the workbench stays available without a systemd user service.

An optional systemd user unit runs that same product command: after `isos init --mode product` and configuring verified core and UI paths, run `./install.sh --install-user-service` to install the CLI and enable and start the unit, or run `tools/user-service.sh install` when the CLI is already installed. Run `tools/user-service.sh remove` to disable it. CLI commands and shell completion work without the unit.


In the second terminal:

`isos open` starts the workbench if needed, prints the UI URL, and opens the browser without forwarding desktop toolkit diagnostics to the terminal.

```sh
isos open
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets

# Add the installed MuJoCo node set in the Graph workbench, then Validate and Apply.
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run` starts the workbench if absent, prepares the saved graph through on-demand core adapters, then starts configured cores in dependency order. It stays attached. Press Ctrl+C to stop the deployment in reverse dependency order. Build source changes in the owning repository and update its configured path when needed. Use `isos status --json`, `isos version check`, `isos services status`, or `isos stop` from another terminal.

`isos init` creates the editable graph only when absent. Home shows package readiness and revisions. Graph edits the one `2.0.0` IsOS/EOS/EGIA graph with versioned node types. Check graph validates a draft; Save graph applies a revision and generates core configuration; Run rechecks the exact package, graph, scene, executable, assets, and MuJoCo dependency. Install or link alone never changes an active deployment. EOS and EGIA have separate workbench pages, with EGIA runtime controls reserved for later.

New graphs contain EOS process nodes and the dispatcher; the operator adds installed package node sets in Graph before applying. Existing authored graphs are preserved. For an older MuJoCo graph, node-set preview can explicitly replace its legacy backend, hardware, and dependent controls before the operator applies the returned draft.

IsOS uses `~/.config/isos/deployments/<name>.nodes.json`, `~/.local/share/isos/packages/`, and `$XDG_RUNTIME_DIR/isos/` by default. `ISOS_HOME` creates isolated `config/`, `data/`, and `run/` directories for testing. Owner-only rotating JSON logs live under `~/.local/share/isos/logs/<deployment>/`; `GET /api/isos/v2/logs` and the UI show the unified stream by default. `[logging]` in `config/isos.toml` controls the default source, recording level, per-file rotation, backup count, and source-file cap. The UI defaults to displaying `info` and higher. Service TOML files and their CLI contract are described in [services](docs/isos/interfaces/services.md).

To use an archive instead of a local link, run `isos pkg verify <manifest> <asset>` and `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` for a local candidate. Approved catalog packages can use `isos pkg install island/mujoco-sim --version 2.0.0` when published. `isos pkg remove` rejects packages referenced by an applied graph.

See [backend design](docs/isos/design/backend-v2.md), [graph interface](docs/isos/interfaces/node-graph-v2.md), [local API](docs/isos/interfaces/local-api.md), and [Chinese README](README_cn.md). Development checks are `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`; save integration output under `logs/test/<run-id>/`.
