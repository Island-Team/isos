# IsOS development guide

Read `README.md`, `docs/isos/README.md`, and `docs/isos/conventions.md` before changing IsOS. Keep English and Chinese documentation aligned. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` before committing. Use Conventional Commits and save integration output in ignored `logs/test/<run-id>/`.

The shared graph at `config/deployments/<name>.nodes.json` is the v2 source of truth. `aether-profile validate --file` admits its node types, ports, and placement. IsOS then resolves package-backed process nodes and projects direct children. The MuJoCo package is an external executable bundle; IsOS selects and verifies the pinned scene and package, while EOS owns the MuJoCo child process and its socket. EOS reads the graph and connects its adapters as clients. Never make EOS install packages or mutate the graph.

`isos init` and `isos serve` keep the workbench and manager in the foreground. `isos run`, `start`, and `restart` stay attached until Ctrl+C, which stops the deployment. Preserve this lifecycle and test both signal paths with a clean home.

`eos.process.backend@v1.0` configures the EOS-owned simulator; `isos.process.component@v1.0` configures IsOS-owned component processes. Keep the package descriptor (`backend-v2.json`) separate from node configuration and the release manifest. Preflight executable, scene references, MuJoCo library, sockets, device IDs, dependency order, and link fingerprint before launching. Stop dependents before providers. The Rust IsOS CLI, owner-only API, and direct-child manager are the v2 main chain. The old systemd component code remains a side chain until its remaining callers are removed.

IsosUI owns frontend source and static builds. Do not copy it into this repository. The UI applies revisions through the same IsOS node-graph API used by the CLI. Graph layout is visual state; node settings and typed ports are runtime state.

The v2 starter graph declares EOS manual, primitive, and skill control nodes with typed status and observation inputs. Keep the dispatcher as the sole actuator command route. Every control target and ordered skill step must be explicit in settings and references; IsOS must not synthesize hidden controls at run time.

`isos init` starts the IsosUI repository's `npm start` command as a supervised process on `127.0.0.1:16001` and runs only the IsOS API on `127.0.0.1:16000`. The source path comes from `ISOSUI_REPO_PATH` or sibling `../IsosUI`; npm dependencies must be installed there. Both servers and the manager stay attached to the foreground command, and Ctrl+C stops them. Never serve UI files from the IsOS API port. Preserve strict loopback Host and Origin checks, and load local `.env` before resolving settings.
