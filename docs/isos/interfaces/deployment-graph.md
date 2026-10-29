# Deployment graph

[中文](../../../docs_cn/isos/interfaces/deployment-graph_cn.md)

IsOS stores one authoritative, formatted JSON graph per deployment at ~/.config/isos/deployments/<name>.json. schema_version is currently 1. The graph is also the editing contract for the future WebUI; layout is optional UI-only data and never changes process behavior.

The fastest path is:

    isos config init
    isos config show
    isos config validate ~/.config/isos/deployments/default.json
    isos config apply ~/.config/isos/deployments/default.json

The generated stack.toml and graph.json are under ~/.local/share/isos/generated/<name>/. Apply writes them and the selected graph atomically at the file level. It does not restart a running stack. The active process state retains a snapshot of the graph used at startup.

## Node fields

| Field | Meaning |
| --- | --- |
| id | Lowercase slug, unique in the graph |
| kind | component, service, backend, or skill |
| role | eos or egia, required only for component nodes |
| enabled | Whether the node participates; defaults to true |
| owned | Whether IsOS starts and stops the process; defaults to true |
| package | Optional exact id and version from the installed package store |
| launch | Command array, optional ready_tcp, timeout_ms, and string environment |
| contract | Protocol contract; enabled EOS and EGIA must match exactly |
| settings | Typed adapter settings, including eos_config and north_http_url |

An enabled EOS node requires settings.north_http_url in http://host:port form so isos eos enter can attach. An enabled IsOS-owned EGIA node requires --managed --no-tui in launch.command. A service or backend with settings.sidecar_of = "eos" or "egia" is included when that component starts; it must depend on the component through an edge.

An enabled owned component, service, or backend requires launch.command. An external service uses owned = false; IsOS never stops it. A skill has no process launch. The graph can reference a package installed from the approved catalog or linked from a development checkout. The package must be installed before start.

The model-service adapter from Island-models is launched as a service node:

    {
      "id": "vision-service",
      "kind": "service",
      "launch": {
        "command": ["/path/to/Island-models/launch.sh", "--managed", "--only", "vision"],
        "ready_tcp": "127.0.0.1:18081"
      }
    }

Run its catalog and preflight commands before enabling it:

    ./launch.sh --catalog-json
    ./launch.sh --only vision --preflight-json

Managed mode never prompts for or downloads weights. A service can supply several EOS model bindings; put those bindings in settings and reference the same service node with model_binding edges.

## Edges

Edges point from consumer to provider: from starts after to. Every edge kind participates in ordering and lifecycle closure.

| Kind | Provider type | Meaning |
| --- | --- | --- |
| depends_on | Any | Required process or component |
| model_binding | service | Model provider for a consumer |
| backend_attachment | backend | Hardware or simulator provider |
| skill_load | skill | Skill package loaded by a consumer |

EGIA must have an edge to EOS when both are enabled. Cycles, missing nodes, disabled providers, duplicate edges, and incompatible contracts are rejected. All enabled nodes start in dependency order and stop in reverse order. Stopping EOS also stops nodes that depend on it, then stops only prerequisites that no remaining active node uses.

## EOS and EGIA example

Replace paths with installed paths on the machine. The EOS north HTTP URL is the address to which eos-cli attaches; it is separate from EOS's gRPC port. EGIA requires --managed --no-tui under IsOS supervision. Managed EGIA refuses to start its own sandbox; declare that as a backend node instead.

    {
      "schema_version": 1,
      "name": "default",
      "nodes": [
        {
          "id": "eos",
          "kind": "component",
          "role": "eos",
          "contract": "island.eos.v1",
          "launch": {
            "command": ["/path/to/Island-eos/bin/eosd", "--config",
                        "/path/to/Island-eos/etc/eos/eos.toml"],
            "ready_tcp": "127.0.0.1:16003"
          },
          "settings": {
            "eos_config": "/path/to/Island-eos/etc/eos/eos.toml",
            "north_http_url": "http://127.0.0.1:16005"
          }
        },
        {
          "id": "eos-north-http",
          "kind": "service",
          "settings": {"sidecar_of": "eos"},
          "launch": {
            "command": ["/path/to/Island-eos/bin/eos-north-http", "--config",
                        "/path/to/Island-eos/etc/ext/north-http/config.toml"],
            "ready_tcp": "127.0.0.1:16005"
          }
        },
        {
          "id": "egia",
          "kind": "component",
          "role": "egia",
          "contract": "island.eos.v1",
          "launch": {
            "command": ["/path/to/Island-egia/bin/egia", "--managed", "--no-tui",
                        "--etc", "/path/to/Island-egia/etc/egia",
                        "--island-config", "/path/to/Island-egia/etc/island.toml",
                        "--mode", "default"],
            "ready_tcp": "127.0.0.1:16002"
          }
        }
      ],
      "edges": [
        {"from": "egia", "to": "eos", "kind": "depends_on"},
        {"from": "eos-north-http", "to": "eos", "kind": "depends_on"}
      ]
    }

The example is a starting point. Verify the installed runtime's actual EOS HTTP port and EGIA mode before applying it. The EOS north HTTP projection is a sidecar, so isos eos start includes it after eosd and isos eos enter can attach. For external backends and model services, add their nodes and edges; do not put their commands in EOS prelaunch configuration.

Related: [local manager API](local-api.md), [release manifest](releases.md).
