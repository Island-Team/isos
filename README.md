# IsOS

IsOS owns the shared Aether v2 graph, backend packages, local workbench API, and direct-child process lifecycle. MuJoCo is the first backend package. EOS reads the graph, starts its verified MuJoCo child, and connects typed adapters to it. EGIA common nodes are available for graph design; they do not start an EGIA process yet.

## Start a MuJoCo deployment

Run `./install.sh` from the v2 `master` checkout to install the CLI without installing or starting a systemd user unit. `isos init --mode dev` creates the configuration and starter graph and exits; running `isos init` again preserves the mode and authored graph. Use `isos init --mode product` to select product admission rules. `isos serve` runs the manager, API, and selected static IsosUI in the foreground; `isos serve --ui-live` explicitly uses supervised Webpack in dev mode.

Edit the tracked [config/isos.toml](config/isos.toml) for portable runtime defaults; [config/isos.example.toml](config/isos.example.toml) shows optional fields. Relative source paths are resolved from this IsOS checkout, regardless of the command directory. Environment variables override file values; `ISOS_CONFIG` selects another absolute config file. Private deployment graphs and build selections remain under `~/.config/isos/`. IsOS does not load `.env` from the command directory. In dev mode, stage immutable builds and select them explicitly:

```sh
isos init --mode dev
isos build eos
isos version select eos <staged-commit>
isos build isosui
isos version select isosui <staged-commit>
isos serve
```

`isos build isosui` and `isos build all` use a sibling `IsosUI` checkout beside the IsOS source by default. Set the absolute `ISOSUI_REPO_PATH` or `[paths].ui_repo` in `config.toml` for another checkout. `build all` checks for the UI source before starting any component build; it stages builds but does not select them.

`isos serve` hosts IsosUI at `http://127.0.0.1:16001/en/isos` and the API at `http://127.0.0.1:16000/api/isos/v2`. `ISOSUI_PORT` and `ISOS_HTTP_PORT` change the loopback ports. Product mode serves only the selected verified static UI and does not need npm or source checkouts. Ctrl+C or `isos stop` stops the UI, API, deployment, explicitly started services, and manager. `isos deployment stop` stops the graph only.

An optional systemd user unit runs that same product command: after `isos init --mode product` and selecting verified builds, run `./install.sh --install-user-service` to install the CLI and enable and start the unit, or run `tools/user-service.sh install` when the CLI is already installed. Run `tools/user-service.sh remove` to disable it. CLI commands and shell completion work without the unit.
Product mode requires an exact selected IsOS executable. With the manager stopped, run `isos build isos` and `isos version select isos <staged-commit>` before `isos serve`.

In the second terminal:

`isos open` requires a running `isos serve` manager. It prints the UI URL and launches the browser without forwarding desktop toolkit diagnostics to the terminal.

```sh
isos open
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets

# Add the installed MuJoCo node set in the Graph workbench, then Validate and Apply.
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run` (also `start` and `restart`) stays attached. Press Ctrl+C to stop the deployment in reverse dependency order. Use `isos status --json`, `isos version check`, `isos services status`, or `isos stop` from another terminal.

`isos init` creates the editable graph only when absent. Home shows package readiness and revisions. Graph edits the one `2.0.0` IsOS/EOS/EGIA graph with versioned `v1.0` node types. Validate and Apply save a revision; Run rechecks the exact package, graph, scene, executable, assets, and MuJoCo dependency. Install or link alone never changes an active deployment. EOS and EGIA have separate workbench pages, with EGIA runtime controls reserved for later.

New graphs contain EOS process nodes and the dispatcher; the operator adds installed package node sets in Graph before applying. Existing authored graphs are preserved. For an older MuJoCo graph, node-set preview can explicitly replace its legacy backend, hardware, and dependent controls before the operator applies the returned draft.

IsOS uses `~/.config/isos/deployments/<name>.nodes.json`, `~/.local/share/isos/packages/`, and `$XDG_RUNTIME_DIR/isos/` by default. `ISOS_HOME` creates isolated `config/`, `data/`, and `run/` directories for testing. Owner-only rotating JSON logs live under `~/.local/share/isos/logs/<deployment>/`; `GET /api/isos/v2/logs?source=all` reads the unified stream with a bounded cursor. The UI defaults to displaying `warn` and higher while recording follows `[logging].level`. Service TOML files and their CLI contract are described in [services](docs/isos/interfaces/services.md).

To use an archive instead of a local link, run `isos pkg verify <manifest> <asset>` and `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` for a local candidate. Approved catalog packages can use `isos pkg install island/mujoco-sim --version 2.0.0` when published. `isos pkg remove` rejects packages referenced by an applied graph.

See [backend design](docs/isos/design/backend-v2.md), [graph interface](docs/isos/interfaces/node-graph-v2.md), [local API](docs/isos/interfaces/local-api.md), and [Chinese README](README_cn.md). Development checks are `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`; save integration output under `logs/test/<run-id>/`.
