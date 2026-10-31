# IsOS integration plan

[中文](../../../docs_cn/isos/maintenance/plan_cn.md)

This page tracks the current IsOS boundary. The [v1 graph](../interfaces/node-graph-v1.md), [service contract](../interfaces/services.md), [release manifest](../interfaces/releases.md), and [local API](../interfaces/local-api.md) define the active interfaces.

## Current ownership

| Owner | Current responsibility |
| --- | --- |
| IsOS | Configured core paths and exact package receipts, shared v1 graph admission, owner-only manager, direct EOS/bridge/config children, static IsosUI, independent explicit services, logs |
| EOS | Runtime, dispatcher and hardware admission, direct MuJoCo simulator child |
| EGIA | Common nodes in graph design; runtime control is reserved |
| Island-models | Catalog, preflight, and managed per-alias launcher; IsOS can adapt an explicit service file |
| Island Port | Reviewed release metadata and account authentication |

`isos init` creates configuration and a generic graph, then exits. `isos serve` runs the API, configured UI bundle, and manager; `isos run` starts the applied graph and stays attached. `isos stop` shuts down all IsOS-owned work; `isos deployment stop` scopes the shutdown to the graph. Services use versioned TOML files outside the graph and start only through `isos services start <id>`. Product launch uses configured, verified artifacts and no source build or npm.

EOS, EGIA, and IsosUI own their builds. IsOS uses one configured binary directory per core and one static UI bundle; its v2 build and commit selection commands are retired. `isos version check` reports each core's product version and launch readiness. Apply generates core configuration from the saved graph revision; Run rechecks the revision, packages, and binaries. MuJoCo is an external IsOS package: package install or link alone does not activate graph changes.

## Remaining work

Qualify real MuJoCo and Island-models deployments on each supported target; publish approved release assets; establish tagged cross-repository API and service-contract compatibility; and complete EGIA runtime integration. Add a typed graph log node and versioned IPC logging contract later; the current owner-only `log_append` operation is a small local interface for selected events. A future graph design may represent services, but the current service manager must remain independent of EOS and EGIA node dependencies.
