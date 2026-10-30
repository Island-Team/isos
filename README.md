# IsOS

IsOS owns the shared Aether v2 graph, backend packages, local workbench API, and direct-child process lifecycle. MuJoCo is the first backend package. EOS reads the graph and connects to MuJoCo as a typed socket client. EGIA common nodes are available for graph design; they do not start an EGIA process yet.

## Start a MuJoCo deployment

Build and install IsOS, the IsosUI local bundle (`npm run build:local`), and the Aether Rust executables (`aether-profile`, `aether-runtime`, `aether-http-bridge`, `aether-config-server`). Put the Aether binaries beside `isos`, on `PATH`, or set `ISOS_AETHER_BIN_DIR` to their absolute build directory before `isos init`. Set `ISOSUI_REPO_PATH` to the built IsosUI checkout or `ISOS_WEB_DIST` to its `dist/local` directory; IsOS also reads these from a `.env` file in the command's working directory. Build the MuJoCo simulator and stage a v2 bundle with `tools/build-backend-bundle.py` in its repository. Set `MUJOCO_ROOT` to the installed MuJoCo runtime, then:

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle
isos init
```

`isos init` serves the local frontend and API at `http://127.0.0.1:16000/en/isos` without a URL token. It keeps the workbench and process manager attached to this terminal. Leave it running while you use a second terminal for graph editing and deployment commands. Press Ctrl+C to stop the server and any managed children.

In the second terminal:

```sh
isos open
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run` (also `start` and `restart`) stays attached. Press Ctrl+C to stop the deployment in reverse dependency order. Use a third terminal for `isos status --json`, `isos logs simulator`, or `isos stop` while it runs.

`isos init` creates the editable graph only when absent and starts the local IsOS workbench server. Home shows package readiness and revisions. Graph edits the one `2.0.0` IsOS/EOS/EGIA graph with versioned `v1.0` node types. Validate and Apply save a revision; Run rechecks the exact package, graph, scene, executable, assets, and MuJoCo dependency. Install or link alone never changes an active deployment. EOS and EGIA have separate workbench pages, with EGIA runtime controls reserved for later.

When a v2 MuJoCo bundle is already linked or installed, a new graph selects its pinned version and first advertised scene. Existing authored graphs are preserved.

IsOS uses `~/.config/isos/deployments/<name>.nodes.json`, `~/.local/share/isos/packages/`, and `$XDG_RUNTIME_DIR/isos/` by default. `ISOS_HOME` creates isolated `config/`, `data/`, and `run/` directories for testing. The owner-only control socket is in the runtime directory; the browser API listens on `127.0.0.1:16000`. A local IsosUI bundle may also be pinned separately.

To use an archive instead of a local link, run `isos pkg verify <manifest> <asset>` and `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` for a local candidate. Approved catalog packages can use `isos pkg install island/mujoco-sim --version 2.0.0` when published. `isos pkg remove` rejects packages referenced by an applied graph.

See [backend design](docs/isos/design/backend-v2.md), [graph interface](docs/isos/interfaces/node-graph-v2.md), [local API](docs/isos/interfaces/local-api.md), and [Chinese README](README_cn.md). Development checks are `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`; save integration output under `logs/test/<run-id>/`.
