# IsOS development guide

Read `README.md`, `docs/isos/README.md`, and `docs/isos/conventions.md` before changing IsOS. Keep English and Chinese documentation aligned. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` before committing. Use Conventional Commits and save integration output in ignored `logs/test/<run-id>/`.

The shared graph at `config/deployments/<name>.nodes.json` is the v2 source of truth. `aether-profile validate --file` admits its node types, ports, and placement. IsOS then resolves package-backed process nodes and projects direct children. The MuJoCo package is an external executable bundle; IsOS selects the pinned scene and owns its process and socket. EOS reads the graph and connects as a client. Never make EOS install packages, mutate the graph, or launch MuJoCo.

`isos init` and `isos serve` keep the workbench and manager in the foreground. `isos run`, `start`, and `restart` stay attached until Ctrl+C, which stops the deployment. Preserve this lifecycle and test both signal paths with a clean home.

`isos.process.backend@v1.0` and `isos.process.component@v1.0` are IsOS-owned types. Keep the package descriptor (`backend-v2.json`) separate from node configuration and the release manifest. Preflight executable, scene references, MuJoCo library, sockets, device IDs, dependency order, and link fingerprint before launching. Stop dependents before providers. The Rust IsOS CLI, owner-only API, and direct-child manager are the v2 main chain. The old systemd component code remains a side chain until its remaining callers are removed.

IsosUI owns frontend source and static builds. Do not copy it into this repository. The UI applies revisions through the same IsOS node-graph API used by the CLI. Graph layout is visual state; node settings and typed ports are runtime state.

`isos init` serves a local IsosUI build from `ISOS_WEB_DIST`, `ISOSUI_REPO_PATH/dist/local`, or a pinned bundle. It keeps the server and manager in the foreground. The loopback UI/API uses its address and port without a URL or bearer token; preserve Host and Origin checks. Load local `.env` before resolving these settings. Do not fall back to the hosted editor for `isos open`.
