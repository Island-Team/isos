# Shared node graph and IsOS process nodes

[中文](../../../docs_cn/isos/interfaces/node-graph_cn.md)

Find each IsOS-owned kind by `core → layer → group → node` in the [node catalog](../layers/README.md). This page remains the authority for serialized fields, validation and compatibility.

**Implementation status:** The `1.0.0` graph and `@v1.*` types below are implemented. The `0.1.0`/`@v0.1` contract and compatibility reader in the final sections are the release target, not current behavior. Product versions, graph versions, node type IDs, HTTP paths, and simulator socket protocol are independent.

One `1.0.0` graph at `~/.config/isos/deployments/<name>.nodes.json` (or `ISOS_HOME/config/deployments/`) configures IsOS, EOS, and the design-only EGIA nodes. A node type declares its core, layer, group, fields, and ports; an instance supplies its ID and settings. Node type versions are independent; the IsOS managed process type is `v1.1` and current EOS hardware types are `v1.0`. Run `aether-profile validate --file <graph>` before apply and again before run. IsOS validates the package projection after registry admission. The graph SHA-256 is the saved revision; an active process set retains its revision until restart.

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

## Current EOS control nodes and legacy skill compatibility

Existing `1.0.0` graphs may contain `manual_0`, `base_velocity_0`, `arm_joint_0`, and `demo_0` under `cores/eos/layers/control`. Their historical types are `eos.control.manual@v1.0`, `eos.control.primitive@v1.0`, and `eos.control.skill@v1.0`. `demo_0` and `eos.control.skill@v1.0` are **compatibility-only**: new `0.1.0` graphs do not author a graph skill sequence or workflow node. These nodes describe configured command sources; runtime requests and outcomes remain separate typed data. Each control references the canonical EOS dispatcher path. Manual and primitive nodes also reference their target hardware node; a skill references its primitive steps. These references do not create an actuator route: the existing dispatcher output remains the only hardware command input.

| Setting | Rust type | Unit | Purpose and validation |
| --- | --- | --- | --- |
| Manual `target` | `String` | canonical node path | Required base or manipulator target; must name an admitted hardware node and appear in `references`. |
| Primitive `target` | `String` | canonical node path | Required hardware target; must name an admitted base or manipulator node and appear in `references`. |
| Primitive `movement_type` | `String` enum | movement mode | Required `body_velocity` for base or `joint_positions` for manipulator; must be supported by the target. |
| Legacy skill `steps` (compatibility-only) | `Vec<String>` | ordered canonical node paths | Required ordered primitive references; each path must be present in `references` and resolve to a registered primitive node. |

Manual and primitive nodes require a `status: Status` input from the dispatcher and a typed observation input from their target: `base_observation: BaseObservation` or `manipulator_observation: ManipulatorObservation`. Skill nodes require `status: Status` from the dispatcher. Manual and primitive nodes expose `request: ControlRequest` and `outcome: CommandOutcome` outputs; skill nodes expose `outcome: SkillOutcome`. The node ID is the final type name plus an index by default; `manual_0` is a current example, while `demo_0` is a legacy compatibility example. It is a graph identity, not a movement payload.

See [backend package design](../design/backend.md) and [local API](local-api.md).

## Canonical node paths and primitive bindings


`eos.dispatcher.primitive_base@v1.0` and `eos.dispatcher.primitive_manipulator@v1.0` are binding nodes. Their `settings` identify `primitive_name`, `primitive_version`, and `movement_type` (strings). Hardware `inputs.command` is an array of `{node,output}` edges from bindings; each binding output must reach one compatible hardware node. The adapter implementation is selected on the hardware instance through `adapter.type`, independently of its generic hardware node type. The IsOS Vision profile is `isos.ui.vision@v1.0` under `isos/ui/panel`; it is an internal UI node, not a supervised process.

## Release target: graph `0.1.0` and compatible reads — not implemented

Newly authored graphs write `version: "0.1.0"`; new type registrations use `@v0.1`. Existing saved `1.0.0` graphs and their registered `@v1.0`/`@v1.1` types remain readable for the safe behavior they already provide. There is no blanket conversion of type IDs: a legacy graph retains its exact IDs, and an unavailable type reports a path-specific migration diagnostic. In particular, the old arbitrary-command `isos.process.component@v1.0` is never executed. Its replacement is a role-selected `isos.process.component@v0.1` with `role` and bounded `args` as above. Historical wire routes, protobuf packages, release-manifest schema v4, and MuJoCo socket protocol `2.0.0` do not change.

| Graph field | JSON type / unit | Owner and purpose | Default and validation | Compatible extension |
| --- | --- | --- | --- | --- |
| `version` | string / graph format | IsOS selects parser and writer | Required `0.1.0` for new drafts; reader also accepts `1.0.0` | No implicit rewrite of saved legacy bytes |
| `robot.id`, `robot.name` | strings / — | Operator identity and display name | Required nonblank strings | Additive metadata only |
| `cores` | object keyed by core ID / — | IsOS graph root; each core owns its subtree | Required, registered keys; unknown core fails admission | New core requires registered owner |
| `layers`, `groups`, `nodes` | nested objects keyed by path segment / — | Core owns placement; type registry validates it | Registered segments; unique four-part `core/layer/group/node` path | New registered placements may be added |
| `node.id` | string / path segment | Instance identity; equals its `nodes` key | Required valid segment | Never silently rename |
| `node.node_type` | string / versioned ID | Registry selects behavior | Required registered ID at declared placement | Existing IDs retain meaning |
| `node.label`, `node.description` | strings / — | Operator-facing instance title and function | Required; description may be empty on legacy instances but new nodes require nonblank text | Existing descriptions preserved |
| `node.package`, `node.adapter.package` | `{id,version}` / exact package pin | IsOS resolves contributed type or adapter | Required for a contributed type or selected package adapter; ID is `namespace/name`, version numeric `x.y.z` | Pin changes require Apply |
| `node.settings` | object / type-specific | Node type owner declares settings; IsOS validates package selectors | Empty object when no settings; unknown settings rejected by type contract | Optional setting additions have defaults |
| `node.inputs` | object of port names to source or source array / — | Registry validates typed connections | Empty when no inputs; each source must resolve to an output with the exact same registered `data_type` ID, cardinality must match; schema `x-unit` annotations do not authorize conversion | Optional edge metadata below |
| `node.references` | string array / canonical paths | Owner declares non-data dependencies | Empty by default; paths exist, unique where type requires | Additive references must not create actuator routes |
| `node.adapter`, `capabilities`, `health_timing`, `observation`, `movement`, `frame_id`, `joint_names` | typed objects/arrays/strings / type-specific | EOS hardware profile fields | Follow the [EOS hardware interface](https://github.com/Island-Team/Island-eos) (`docs/eos/interfaces/hardware.md`); IsOS forwards without inventing hardware meaning | EOS defines compatible defaults |

The registered **type** has a separate, nonblank `description` for new `@v0.1` types. It explains function, inputs, outputs, and operator-visible effects. Old verified sidecars lacking this field receive a generated catalog-only fallback; IsOS and EOS never modify the sidecar bytes. A setting's optional `selector` is defined by the [EOS node contribution contract](https://github.com/Island-Team/Island-eos) (`docs/eos/interfaces/node-contributions.md`); IsOS owns the [resolved options response](local-api.md#release-target-setting-options--not-implemented).

Each `node.inputs.<port>` source adds optional scheduling metadata:

| Source field | JSON type / unit | Purpose | Default and validation | Compatibility |
| --- | --- | --- | --- | --- |
| `node` | string / canonical node path | Output owner | Required existing source node | Unchanged |
| `output` | string / port name | Output port | Required registered output compatible with destination data type | Unchanged |
| `priority` | integer / Linux nice scale | Weighted delivery order | 0; inclusive `-20..19`, lower number has higher priority | Absent legacy value normalizes to 0 |
| `frequency_hz` | number / Hz | Maximum delivered update rate on this edge | Absent means no edge cap; if present finite and `>0` | Absent legacy edge remains uncapped |

Example **source-object fragment** for a compatible input port (target JSON, not yet accepted by current binaries):

```json
{"node":"eos/dispatcher/base/velocity","output":"command","priority":5,"frequency_hz":15.0}
```

Place this object inside a compatible destination's `inputs.command` array after the source binding is declared; the [current starter template](../../../templates/generic-graph.toml) shows a complete graph without target edge fields. The producer's `expected_rate_hz` is a registry hint, independent of this edge cap. EOS, as the route owner, uses a monotonic token bucket per active edge and weighted fair delivery over bounded queues. Observation overflow increments a visible counter and emits a rate-limited warning; a command that cannot be admitted is explicitly rejected or reported uncertain, never silently dropped. Stopping a route discards non-command queued work and invokes dispatcher stop for actuators. Scheduling priority never changes command authorization: actuator data still passes the dispatcher. IsOS stores and validates edge metadata; it does not schedule hardware itself. IsosUI renders both values on a wire and identifies design-only edges that have no runtime route.

Reader algorithm: select the parser from `version`; validate its original type IDs and safe semantics; normalize missing optional fields in memory; calculate the revision from the **original saved bytes**; validate packages and routes; render generated configuration without rewriting the graph. `Apply` on a newly authored graph writes normalized `0.1.0` JSON only after a matching revision. A separate explicit migration operation must preview a diff, require the old revision, write atomically, and return the new revision; it never changes the active revision until Restart. Unknown future graph versions fail with a supported-version diagnostic. See the [architecture algorithm](../design/system.md).
