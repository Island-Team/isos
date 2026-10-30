# IsOS-owned backend packages

[中文](../../../docs_cn/isos/design/backend-v2_cn.md)

MuJoCo is the first Aether backend bundle: `island/mujoco-sim@2.0.0`. A bundle contains `backend-v2.json`, `bin/eos-mujoco-sim`, scenes, and assets. The release manifest describes distribution and archive verification. The backend descriptor describes executable, scene IDs, device IDs, protocol, and runtime dependency. The shared `2.0.0` graph contains the configured `isos.process.backend@v1.0` node. These have separate jobs so package installation cannot silently edit a deployment.

IsOS resolves the exact pinned receipt at validate, apply, and run. Local links are reread on every resolution. It checks the descriptor identity, executable permission, selected scene, referenced scene assets, required MuJoCo library, canonical socket path, and EOS adapter device/socket references. A fingerprint of the descriptor, executable, scene, and referenced assets detects changes to a running linked package. The user edits scene selection in the graph and explicitly applies it. EOS receives the validated graph as read-only input, constructs typed camera/base/manipulator adapters, and connects to the simulator's Unix socket. It has no package installation or simulator launch capability.

The socket is below the owner-only IsOS runtime directory. IsOS starts the backend before EOS; the process graph ensures EOS config and runtime children depend on it. On stop or provider failure, IsOS stops EOS dependents first. The manager owns direct child handles, readiness probes, bounded restart, and per-process logs. A backend can be removed only after no applied graph references its exact package ID and version. Later backend packages can use the same out-of-process resolver boundary; actuator types still pass EOS safety admission.

`isos init` writes a starter graph once. Home reports installed/linked bundles, scene and prerequisite issues, saved versus active revision, run status, and logs. Graph edits the one shared IsOS/EOS/EGIA graph. EGIA common nodes are design-only until their runtime exists. Visual layout is separate from runtime configuration.

## Development check

Use a fresh `ISOS_HOME`, link a staged bundle with `isos pkg link island/mujoco-sim 2.0.0 <bundle>`, then run `isos config init`, `isos config validate <graph>`, `isos config apply <graph>`, `isos run`, `isos restart`, `isos logs simulator`, and `isos stop`. Set `MUJOCO_ROOT` to the installed MuJoCo runtime. The EOS `aether-profile` validator and Rust executables must be available. See [node fields](../interfaces/node-graph-v2.md) and [package commands](../usage/packages.md).
