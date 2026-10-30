# Shared v2 graph and IsOS process nodes

[中文](../../../docs_cn/isos/interfaces/node-graph-v2_cn.md)

One `2.0.0` graph at `~/.config/isos/deployments/<name>.nodes.json` (or `ISOS_HOME/config/deployments/`) configures IsOS, EOS, and the design-only EGIA nodes. A node type declares its core, layer, group, fields, and ports; an instance supplies its ID and settings. Current node types are `v1.0`. Run `aether-profile validate --file <graph>` before apply and again before run. IsOS validates the package projection after registry admission. The graph SHA-256 is the saved revision; an active process set retains its revision until restart.

`isos init` writes an editable graph only when absent. It points the EOS camera/base/manipulator adapters at one IsOS backend node. `isos config validate <file>` checks the package and graph; `isos config apply <file>` saves normalized JSON; `isos run`, `restart`, `stop`, `status`, and `logs <node>` use the owner-only direct-child manager. The MuJoCo package starts before EOS and stops after EOS. A package install or link never changes an active deployment.

## Backend node

Path: `cores/isos/layers/process/groups/backend/nodes/<id>`. Type: `isos.process.backend@v1.0`. The node describes configuration; `backend-v2.json` is package metadata, and camera frames and commands have separate Rust payload structs. IsOS derives the executable and scene paths from the verified bundle, never from the node's command field.

| Setting | Rust type | Unit | Purpose and validation |
| --- | --- | --- | --- |
| `package` | `String` | package ID | Required exact ID, initially `island/mujoco-sim`. |
| `version` | `String` | package version | Required exact installed or linked version, initially `2.0.0`. |
| `scene` | `String` | scene ID | Required ID from the descriptor; selects one XML scene. |
| `socket` | `PathBuf` | Unix path | Required owner-only socket under IsOS runtime, absolute and at most 107 bytes. EOS adapters must use this path. |
| `env` | `BTreeMap<String,String>` | environment | Optional runtime values passed only to the simulator child. `MUJOCO_ROOT` locates the library; `MUJOCO_GUI=1` requests the simulator's visible window on a graphical host. Keys are ASCII alphanumeric or `_`, values exclude NUL. |
| `start_timeout_ms` | `u64` | ms | Readiness deadline, default 30000, range 1–120000. |
| `restart` | `Restart` | policy | `never`, `on_failure`, or `always`; default `never`. |
| `max_restarts` | `u32` | attempts | Automatic restart cap, default 0, maximum 10. |
| `depends_on` | `Vec<String>` | canonical node paths | Process prerequisites, unique and acyclic. |

The backend descriptor's `entrypoint`, `scenes`, `devices`, `protocol`, and MuJoCo runtime requirement are verified at apply and run. IsOS checks referenced scene assets and includes their bytes in the link fingerprint. Each EOS hardware adapter must name the backend in `references`, set `adapter.settings.backend` to its canonical path, match `socket_path` to the backend socket, and match `device_id`/`simulator_node` to the package device for its camera/base/manipulator group.
IsOS launches the descriptor entrypoint directly and prepends the verified MuJoCo library directory to that child's `LD_LIBRARY_PATH`, preserving any existing search path. This environment change is scoped to the simulator process.

## Component process node

Path: `cores/isos/layers/process/groups/component/nodes/<id>`. Type: `isos.process.component@v1.0`.

| Setting | Rust type | Unit | Purpose and validation |
| --- | --- | --- | --- |
| `command` | `Vec<String>` | argv | Required absolute executable and up to 63 bounded arguments; no shell. IsOS checks executable presence and execute permission. |
| `cwd` | `Option<PathBuf>` | path | Optional absolute working directory. |
| `env` | `BTreeMap<String,String>` | environment | Optional child variables; keys ASCII alphanumeric or `_`, values exclude NUL. |
| `ready_tcp` | `Option<String>` | host:port | TCP readiness probe; exclusive with `ready_unix`. |
| `ready_unix` | `Option<PathBuf>` | Unix path | Socket readiness probe, absolute and at most 107 bytes; exclusive with `ready_tcp`. |
| `start_timeout_ms` | `u64` | ms | Readiness deadline, default 30000, range 1–120000. |
| `restart` | `Restart` | policy | `never`, `on_failure`, or `always`. |
| `max_restarts` | `u32` | attempts | Automatic restart cap, maximum 10. |
| `depends_on` | `Vec<String>` | canonical node paths | Prerequisites; unique, existing, and acyclic. |

The manager starts direct children in dependency order, waits for readiness, and stops process groups in reverse order with TERM then KILL after two seconds. Provider failure stops dependents before recovery. Logs are `$XDG_DATA_HOME/isos/process-logs/<deployment>/<node>.log`. Status includes saved/active revision, pinned package, process PID/restarts, and last supervisor error. The private Unix manager API retains its schema 1 envelope and uses `v2_start`, `v2_restart`, `v2_stop`, and `v2_status` operations.

See [backend package design](../design/backend-v2.md) and [local API](local-api.md).
