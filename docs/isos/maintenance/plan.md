# IsOS integration plan

[中文](../../../docs_cn/isos/maintenance/plan_cn.md)

This page tracks the current IsOS boundary. The [v2 graph](../interfaces/node-graph-v2.md), [service contract](../interfaces/services.md), [release manifest](../interfaces/releases.md), and [local API](../interfaces/local-api.md) define the active interfaces.

## Current ownership

| Owner | Current responsibility |
| --- | --- |
| IsOS | Exact build and package receipts, shared v2 graph admission, owner-only manager, direct EOS/bridge/config children, static IsosUI, independent explicit services, logs |
| EOS | Runtime, dispatcher and hardware admission, direct MuJoCo simulator child |
| EGIA | Common nodes in graph design; runtime control is reserved |
| Island-models | Catalog, preflight, and managed per-alias launcher; IsOS can adapt an explicit service file |
| Island Port | Reviewed release metadata and account authentication |

`isos init` creates configuration and a generic graph, then exits. `isos serve` runs the API, selected UI, and manager; `isos run` starts the applied graph and stays attached. `isos stop` shuts down all IsOS-owned work; `isos deployment stop` scopes the shutdown to the graph. Services use versioned TOML files outside the graph and start only through `isos services start <id>`. Product launch uses pinned, verified artifacts and no source checkout or npm.

A build stages immutable artifacts. Selection is explicit: EOS selection applies its three graph executable paths at the expected revision, IsosUI selection pins a static build, and IsOS selection activates a staged executable only while the manager is stopped. `isos version check` compares configured, selected, and active identities. Package install or link alone does not activate graph changes.

## Remaining work

Qualify real MuJoCo and Island-models deployments on each supported target; publish approved release assets; establish tagged cross-repository API and service-contract compatibility; and complete EGIA runtime integration. A future graph design may represent services, but the current service manager must remain independent of EOS and EGIA node dependencies. Keep legacy component commands as compatibility paths until their callers are retired.
