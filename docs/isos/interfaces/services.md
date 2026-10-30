# Independent service contract v2

IsOS owns model-like service processes without representing them as deployment graph nodes. The owner-only IsOS manager reads one TOML file per service at `<IsOS config>/services/<id>.toml`. A local CLI or owner-only manager operation is the caller. Services start only when explicitly requested; `isos serve` and `isos run` do not start them. Stopping the whole IsOS manager stops all of its services.

## Definition

```toml
version = "2.0.0"
id = "gateway"
command = ["/absolute/Island-models/launch.sh", "--managed", "--only", "gateway"]
cwd = "/absolute/Island-models"
depends_on = []
restart = "on_failure"
max_restarts = 2
start_timeout_ms = 30000
stop_timeout_ms = 5000

[artifact]
path = "/absolute/Island-models/launch.sh"
sha256 = "<64 hexadecimal SHA-256 digits>"

[readiness]
kind = "tcp"
target = "127.0.0.1:18003"

[adapter]
kind = "island_models"
repo = "/absolute/Island-models"
alias = "gateway"
```

| Field | Type and unit | Purpose and validation |
| --- | --- | --- |
| `version` | string | Must be `2.0.0`. |
| `id` | string | 1–32 ASCII letters, digits, `_`, or `-`; matches file stem. |
| `artifact.path`, `artifact.sha256` | absolute file path, SHA-256 hex | Exact artifact identity; content is hashed before launch. For Island-models this is `launch.sh`. |
| `adapter` | optional tagged table | `kind = "island_models"` verifies `repo` (absolute path) and `alias` against the installed catalog and preflight; its result must equal `command`. |
| `command` | string array | Absolute executable and up to 63 bounded arguments. The executable must be executable. |
| `cwd` | absolute directory | Process working directory; must exist. |
| `env_from` | map of environment names | Maps a child variable to a variable already in the IsOS manager environment; required source must be set. Values are never stored in the file or shown in status. |
| `preflight`, `readiness`, `health` | probe tables | Optional admission, startup, and running health checks. `kind` is `tcp`, `unix`, or `exec`; `target` is an address, absolute socket path, or absolute executable path. `args` is allowed for `exec`. |
| `depends_on` | service ID array | Other service definitions only; cycles and missing IDs fail validation. Starting a service starts its dependencies first. |
| `restart`, `max_restarts` | enum, count | `never`, `on_failure`, or `always`; at most 10 automatic restarts. |
| `start_timeout_ms`, `stop_timeout_ms` | milliseconds | Startup readiness deadline 1–120000; SIGTERM grace 1–60000 before SIGKILL. |

The manager verifies all requested definitions and artifacts before starting any child. It starts dependencies in order, rolls back newly started children if startup fails, and stops dependents before providers. A stopped service remains stopped until explicitly started or restarted. Status reports ID, state, PID, restart count, active artifact SHA-256, and optional health result. Logs are captured into private rotating JSON line files with source `service-<id>` and read through a bounded cursor. A child process belongs to a dedicated process group; stop signals the group, waits for the configured grace, then kills it if necessary.

The public CLI is `isos services list|validate|start <id>|stop <id>|restart <id>|status [id]|logs <id>`. Mutations use the owner-only manager socket, which rejects callers with a different UID. `list` and `validate` read files without starting services. A repeated `start` returns `{"started":[]}`; a repeated `stop` returns `{"stopped":[]}`. `services logs` returns `lines`, `next_cursor`, and `rotated`, matching the IsOS log cursor contract.

## Island-models adapter

The adapter reads `<repo>/launch.sh --catalog-json`, requires schema version 1 and the requested alias, then runs `--only <alias> --preflight-json`. It requires `ready:true` before returning the absolute `launch.sh --managed --only <alias>` command. IsOS does not edit Island-models or adopt its processes by port. Keep the local service definition pinned to the launcher digest; rerun validation when updating that repository. The adapter reports missing artifacts and errors from preflight, but does not expose environment values.

`isos services adapter models <absolute-repo> <alias>` performs this check and prints the managed command. It does not write a service file; the operator creates and pins the TOML definition explicitly.

## Errors

An unsupported version, malformed ID, missing environment reference or dependency, cycle, digest mismatch, failing preflight, failed readiness, unexpected exit, or unhealthy probe is observable as a manager error and IsOS warning log. A failed start stops only processes started by that attempt. Repeating `stop` for an already stopped service succeeds with an empty `stopped` list.

[中文](../../../docs_cn/isos/interfaces/services_cn.md)
