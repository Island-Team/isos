# IsOS

IsOS owns the shared Aether v2 graph, backend packages, local workbench API, and direct-child process lifecycle. MuJoCo is the first backend package. EOS reads the graph, starts its verified MuJoCo child, and connects typed adapters to it. EGIA common nodes are available for graph design; they do not start an EGIA process yet.

## Start a MuJoCo deployment

Run `./install.sh` from the v2 `master` checkout to replace the installed IsOS command. The installer stops the previous deployment and legacy `isos-server.service`, removes its obsolete UI token, and force installs the binary built from this checkout. Check `isos --version` afterward; it must report `2.0.0`. `isos stop` also stops a remaining legacy IsOS server. A v2 `isos init` server runs in its terminal and exits with Ctrl+C.

Build and install IsOS and the Aether Rust executables (`aether-profile`, `aether-runtime`, `aether-http-bridge`, `aether-config-server`). In the sibling IsosUI checkout, run `npm ci` once; `isos init` subsequently starts its `npm start` server. Put the Aether binaries beside `isos`, on `PATH`, or set `ISOS_AETHER_BIN_DIR` to their absolute build directory before `isos init`. Set `ISOSUI_REPO_PATH` if IsosUI is not at `../IsosUI`; IsOS also reads it from a `.env` file in the command's working directory. Build the MuJoCo simulator and stage a v2 bundle with `tools/build-backend-bundle.py` in its repository. Set `MUJOCO_ROOT` to the installed MuJoCo runtime, then:

```sh
isos pkg link island/mujoco-sim 2.0.0 /absolute/path/to/bundle
isos init
```

`isos init` starts IsosUI at `http://127.0.0.1:16001/en/isos` and serves the IsOS API only at `http://127.0.0.1:16000/api/isos/v2`, without a URL token. `ISOSUI_PORT` and `ISOS_HTTP_PORT` can change the ports; both bind only to loopback. Leave the command running while editing the graph and deploying. Ctrl+C stops the npm frontend, API, manager, and managed children.

In the second terminal:

```sh
isos open
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

`isos run` (also `start` and `restart`) stays attached. Press Ctrl+C to stop the deployment in reverse dependency order. Use a third terminal for `isos status --json`, `isos logs eos`, or `isos stop` while it runs.

`isos init` creates the editable graph only when absent and starts the local IsOS workbench server. Home shows package readiness and revisions. Graph edits the one `2.0.0` IsOS/EOS/EGIA graph with versioned `v1.0` node types. Validate and Apply save a revision; Run rechecks the exact package, graph, scene, executable, assets, and MuJoCo dependency. Install or link alone never changes an active deployment. EOS and EGIA have separate workbench pages, with EGIA runtime controls reserved for later.

When a v2 MuJoCo bundle is already linked or installed, a new graph selects its pinned version and first advertised scene. Existing authored graphs are preserved.

IsOS uses `~/.config/isos/deployments/<name>.nodes.json`, `~/.local/share/isos/packages/`, and `$XDG_RUNTIME_DIR/isos/` by default. `ISOS_HOME` creates isolated `config/`, `data/`, and `run/` directories for testing. The owner-only control socket is in the runtime directory; the browser API listens on `127.0.0.1:16000`, while npm serves IsosUI on `127.0.0.1:16001`. The default EOS HTTP bridge and read-only graph catalog listen on `127.0.0.1:16020` and `127.0.0.1:16021`.

To use an archive instead of a local link, run `isos pkg verify <manifest> <asset>` and `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` for a local candidate. Approved catalog packages can use `isos pkg install island/mujoco-sim --version 2.0.0` when published. `isos pkg remove` rejects packages referenced by an applied graph.

See [backend design](docs/isos/design/backend-v2.md), [graph interface](docs/isos/interfaces/node-graph-v2.md), [local API](docs/isos/interfaces/local-api.md), and [Chinese README](README_cn.md). Development checks are `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`; save integration output under `logs/test/<run-id>/`.
