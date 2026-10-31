# Shared v1 graph and IsOS process nodes

[中文](../../../docs_cn/isos/interfaces/node-graph-v1_cn.md)

One `1.0.0` graph at `~/.config/isos/deployments/<name>.nodes.json` (or `ISOS_HOME/config/deployments/`) configures IsOS, EOS, and the design-only EGIA nodes. A node type declares its core, layer, group, fields, and ports; an instance supplies its ID and settings. Current node types are `v1.0`. Run `aether-profile validate --file <graph>` before apply and again before run. IsOS validates the package projection after registry admission. The graph SHA-256 is the saved revision; an active process set retains its revision until restart.

`isos init` writes an editable graph only when absent and exits. The new draft contains EOS process nodes and the dispatcher; preview an installed package node set to add hardware. `isos config validate <file>` checks the package and graph; `isos config apply <file>` saves normalized JSON. `isos serve` runs the owner-only manager and UI. `isos run` and `restart` stay attached; `isos deployment stop` stops only this graph, while `isos stop` shuts down the manager and all owned children. EOS starts MuJoCo before admitting hardware operations and stops it with the EOS runtime. A package install or link never changes an active deployment.

## Backend node

New package-contributed MuJoCo process nodes use `eos.process.mujoco@v1.0` at `eos/process/mujoco/<id>`. Their exact package is `profile.package:{id,version}`; settings contain `scene`, `socket`, optional `env`, and `start_timeout_ms` with the types and limits below. Camera, base, and manipulator devices use generic EOS types at `eos/hardware/camera/<id>`, `eos/hardware/base/<id>`, and `eos/hardware/manipulator/<id>`. Each device selects a package-pinned adapter family in `profile.adapter`; the package does not own the generic hardware node type.


| Setting | Rust type | Unit | Purpose and validation |
| --- | --- | --- | --- |
| `scene` | `String` | scene ID | Required ID from the descriptor; selects one XML scene. |
| `socket` | `PathBuf` | Unix path | Required owner-only socket under IsOS runtime, absolute and at most 107 bytes. EOS adapters must use this path. |
| `env` | `BTreeMap<String,String>` | environment | Optional runtime values passed only to the simulator child. `MUJOCO_ROOT` locates the library; `MUJOCO_GUI=1` requests the simulator's visible window on a graphical host. Keys are ASCII alphanumeric or `_`, values exclude NUL. |
| `start_timeout_ms` | `u64` | ms | Readiness deadline, default 30000, range 1–120000. |

The backend descriptor's `entrypoint`, `scenes`, `devices`, `protocol`, and MuJoCo runtime requirement are verified at apply and run. IsOS checks referenced scene assets and includes their bytes in the link fingerprint. Each EOS hardware adapter must name the backend in `references`, set `adapter.settings.backend` to its canonical path, match `socket_path` to the backend socket, and match `device_id`/`simulator_node` to the package device for its camera/base/manipulator group.
IsOS passes the verified executable and scene as explicit EOS runtime arguments and the verified MuJoCo library path in the EOS child environment. EOS starts the simulator, scopes the backend `env` values to that child, and waits for the configured Unix socket.

## Component process node

Path: `cores/isos/layers/process/groups/component/nodes/<id>`. Current type: `isos.process.component@v1.1`; saved `@v1.0` nodes are rejected with conversion guidance.

| Setting | Rust type | Unit | Purpose and validation |
| --- | --- | --- | --- |
| `role` | `String` | core ID | Required `eos` or `egia` for managed `isos.process.component@v1.1` nodes; selects the configured core binary directory. |
| `args` | `Vec<String>` | argv arguments | Arguments for the binary determined by `role`, at most 63 bounded strings; no shell or executable path. |
| `command` | `Vec<String>` | argv | Obsolete; rejected on Apply. Use `role` and `args`. |
| `cwd` | `Option<PathBuf>` | path | Optional absolute working directory. |
| `env` | `BTreeMap<String,String>` | environment | Optional child variables; keys ASCII alphanumeric or `_`, values exclude NUL. |
| `ready_tcp` | `Option<String>` | host:port | TCP readiness probe; exclusive with `ready_unix`. |
| `ready_unix` | `Option<PathBuf>` | Unix path | Socket readiness probe, absolute and at most 107 bytes; exclusive with `ready_tcp`. |
| `start_timeout_ms` | `u64` | ms | Readiness deadline, default 30000, range 1–120000. |
| `restart` | `Restart` | policy | `never`, `on_failure`, or `always`. |
| `max_restarts` | `u32` | attempts | Automatic restart cap, maximum 10. |
| `depends_on` | `Vec<String>` | canonical node paths | Prerequisites; unique, existing, and acyclic. |

The manager starts direct children in dependency order, waits for readiness, and stops process groups in reverse order with TERM then KILL after two seconds. Provider failure stops dependents before recovery. IsOS diagnostics and selected structured child events are stored in rotating `$XDG_DATA_HOME/isos/logs/<deployment>/<source>.log` files; plain child output is not mirrored. Status includes saved/active revision, pinned package, process PID/restarts, and last supervisor error. The private Unix manager API retains its schema 1 envelope and uses `deployment_start`, `deployment_restart`, `deployment_stop`, and `deployment_status` operations.

## Optional EOS control nodes

An authored graph may declare `manual_0`, `base_velocity_0`, `arm_joint_0`, and `demo_0` under `cores/eos/layers/control`. Their types are `eos.control.manual@v1.0`, `eos.control.primitive@v1.0`, and `eos.control.skill@v1.0`. They describe configured command sources; runtime requests and outcomes remain separate typed data. Each control references the canonical EOS dispatcher path. Manual and primitive nodes also reference their target hardware node; a skill references its primitive steps. These references do not create an actuator route: the existing dispatcher output remains the only hardware command input.

| Setting | Rust type | Unit | Purpose and validation |
| --- | --- | --- | --- |
| Manual `target` | `String` | canonical node path | Required base or manipulator target; must name an admitted hardware node and appear in `references`. |
| Primitive `target` | `String` | canonical node path | Required hardware target; must name an admitted base or manipulator node and appear in `references`. |
| Primitive `movement_type` | `String` enum | movement mode | Required `body_velocity` for base or `joint_positions` for manipulator; must be supported by the target. |
| Skill `steps` | `Vec<String>` | ordered canonical node paths | Required ordered primitive references; each path must be present in `references` and resolve to a registered primitive node. |

Manual and primitive nodes require a `status: Status` input from the dispatcher and a typed observation input from their target: `base_observation: BaseObservation` or `manipulator_observation: ManipulatorObservation`. Skill nodes require `status: Status` from the dispatcher. Manual and primitive nodes expose `request: ControlRequest` and `outcome: CommandOutcome` outputs; skill nodes expose `outcome: SkillOutcome`. The node ID is the final type name plus an index by default, such as `manual_0` or `demo_0`; it is a graph identity, not a movement payload.

See [backend package design](../design/backend-v2.md) and [local API](local-api.md).

## Canonical node paths and primitive bindings


`eos.dispatcher.primitive_base@v1.0` and `eos.dispatcher.primitive_manipulator@v1.0` are binding nodes. Their `settings` identify `primitive_name`, `primitive_version`, and `movement_type` (strings). Hardware `inputs.command` is an array of `{node,output}` edges from bindings; each binding output must reach one compatible hardware node. The adapter implementation is selected on the hardware instance through `adapter.type`, independently of its generic hardware node type. The IsOS Vision profile is `isos.ui.vision@v1.0` under `isos/ui/panel`; it is an internal UI node, not a supervised process.
