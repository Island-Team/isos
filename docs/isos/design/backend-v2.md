# IsOS-owned backend packages

[中文](../../../docs_cn/isos/design/backend-v2_cn.md)

MuJoCo is the first Aether backend bundle: `island/mujoco-sim@2.0.0`. A bundle contains `backend-v2.json`, `bin/eos-mujoco-sim`, scenes, and assets. The release manifest describes distribution and archive verification. The backend descriptor describes executable, scene IDs, device IDs, protocol, and runtime dependency. The shared `2.0.0` graph contains the configured `eos.process.backend@v1.0` node. These have separate jobs so package installation cannot silently edit a deployment.

IsOS resolves the exact pinned receipt at validate, apply, and run. Local links are reread on every resolution. It checks the descriptor identity, executable permission, selected scene, referenced scene assets, required MuJoCo library, canonical socket path, and EOS adapter device/socket references. A fingerprint of the descriptor, executable, scene, and referenced assets detects changes to a running linked package. The user edits scene selection in the graph and explicitly applies it. IsOS passes the verified executable and scene paths to EOS at launch. EOS reads the graph, starts MuJoCo as its child, and connects typed camera/base/manipulator adapters to the simulator socket. EOS cannot install packages or change the applied graph.

The socket is below the owner-only IsOS runtime directory. IsOS starts the EOS runtime first; EOS starts MuJoCo and waits for its socket before admitting hardware operations. IsOS owns the EOS, bridge, and config child handles; EOS owns the simulator child. Stopping EOS terminates MuJoCo, and an unexpected simulator exit fails EOS so IsOS can restart the whole component. A backend can be removed only after no applied graph references its exact package ID and version. Later backend packages can use the same out-of-process resolver boundary; actuator types still pass EOS safety admission.

`isos init` writes a starter graph once. Home reports installed/linked bundles, scene and prerequisite issues, saved versus active revision, run status, and logs. Graph edits the one shared IsOS/EOS/EGIA graph. EGIA common nodes are design-only until their runtime exists. Visual layout is separate from runtime configuration.

## Development check

Use a fresh `ISOS_HOME`, link a staged bundle with `isos pkg link island/mujoco-sim 2.0.0 <bundle>`, then run `isos config init`, `isos config validate <graph>`, `isos config apply <graph>`, `isos run`, `isos restart`, `isos logs eos`, and `isos stop`. Set `MUJOCO_ROOT` to the installed MuJoCo runtime. The EOS `aether-profile` validator and Rust executables must be available. See [node fields](../interfaces/node-graph-v2.md) and [package commands](../usage/packages.md).
