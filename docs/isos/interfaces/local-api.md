# Local manager API

The IsOS manager owns deployment graphs and scoped process state. The CLI calls the same core directly. Local clients can use an owner-only Unix socket, and the hosted WebUI can use a loopback HTTP bridge. API apply saves configuration without restarting a running stack.

[中文](../../../docs_cn/isos/interfaces/local-api_cn.md)

## Authentication and transport

Run isos daemon for the Unix socket at $XDG_RUNTIME_DIR/isos/manager.sock. Its parent directory has mode 0700 and its socket has mode 0600. Each connection sends one JSON request line and receives one JSON response line.

Run isos ui to bind the HTTP bridge to 127.0.0.1:17600. It prints a random 256-bit token that lasts only while that command runs. The WebUI asks the operator to paste it and holds it in memory; clients send Authorization: Bearer <token>. Browser requests are allowed only from https://www.os-island.com or https://os-island.com. A local development origin can be added with isos ui --allow-origin <exact-origin>. The HTTP Host must be 127.0.0.1:17600, using the actual selected port. The bridge does not set cookies or listen on a network interface.

## Shared wire rules

The API version is 1. JSON requests use Content-Type: application/json. A successful operation returns HTTP 200 with:

    {"schema_version":1,"ok":true,"data":{"started":["eos"]},"error":null}

A graph or lifecycle validation failure returns HTTP 400:

    {"schema_version":1,"ok":false,"data":null,"error":"EOS and EGIA require the same explicit protocol contract"}

An absent or wrong token returns HTTP 401. An unapproved browser Origin or Host returns HTTP 403. The unauthenticated health route returns only the service name and version. CORS preflight uses OPTIONS with the approved Origin and returns HTTP 204. If that approved preflight includes Access-Control-Request-Private-Network: true, its response includes Access-Control-Allow-Private-Network: true. A request body larger than 2 MB is rejected.

The deployment query field and JSON field default to ISOS_DEPLOYMENT or default. A role may be eos or egia; omitting it addresses the full deployment. In a graph edge, from is the consumer and to is the provider.

## GET /api/isos/v1/health

This route is unauthenticated and checks whether the local bridge is available. It reveals no graph or token.

    GET /api/isos/v1/health

    {"schema_version":1,"ok":true,"data":{"service":"isos","version":"0.1.0"},"error":null}

## GET /api/isos/v1/status

The paired WebUI reads the selected deployment or component status. A running EOS returns active, north_http_url, running, and per-node state:

    GET /api/isos/v1/status?deployment=default&role=eos

    {"schema_version":1,"ok":true,"data":{"schema_version":1,"active":"default","north_http_url":"http://127.0.0.1:16005","running":true,"nodes":[{"id":"eos","running":true}]},"error":null}

When EOS is offline, active is null, north_http_url is empty, and running is false. Authentication failure is HTTP 401; an invalid role or state is HTTP 400.

## GET /api/isos/v1/graph

The paired WebUI reads the applied graph for editing. If no graph is applied, the route returns HTTP 400 with an explanatory error.

    GET /api/isos/v1/graph?deployment=default

    {"schema_version":1,"ok":true,"data":{"schema_version":1,"name":"default","nodes":[],"edges":[]},"error":null}

## POST /api/isos/v1/apply

The paired WebUI submits the complete graph. Apply validates and formats it, renders activation templates and generated TOML, then saves it. It does not restart active nodes.

    {"schema_version":1,"deployment":"default","graph":{"schema_version":1,"name":"default","nodes":[],"edges":[]}}

    {"schema_version":1,"ok":true,"data":{"applied":"default"},"error":null}

A malformed graph, missing package file, cycle, or contract mismatch returns HTTP 400. The graph schema is documented in [deployment-graph.md](deployment-graph.md).

## POST /api/isos/v1/start

The paired WebUI starts the full deployment or a component dependency closure.

    {"schema_version":1,"deployment":"default","role":"eos"}

    {"schema_version":1,"ok":true,"data":{"started":["model-service","eos"]},"error":null}

Missing components, incompatible packages, unavailable user systemd, or failed readiness return HTTP 400 with an actionable error. Already-running nodes are omitted from started.

## POST /api/isos/v1/stop

The paired WebUI stops deployment-owned nodes in reverse dependency order. A component stop also stops dependents and preserves shared prerequisites required by remaining active nodes.

    {"schema_version":1,"deployment":"default","role":"eos"}

    {"schema_version":1,"ok":true,"data":{"stopped":["egia","eos"]},"error":null}

An inactive deployment succeeds with an empty stopped array. A user-systemd stop failure returns HTTP 400.

## Unix socket operations

The Unix socket has equivalent status, graph, start, stop, and apply operations. Its request envelope includes op and schema_version:

    {"schema_version":1,"op":"status","deployment":"default","role":"eos"}

It returns the same success or error JSON line, without HTTP status codes. The CLI-specific isos eos status --json command returns the unwrapped status data for compatibility with eos-cli.

Related: [deployment graph](deployment-graph.md), [release manifest](releases.md).
