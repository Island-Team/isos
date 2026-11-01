# IsOS 0.x architecture and algorithms

[中文](../../../docs_cn/isos/design/system_cn.md)

**Target design; not implemented as a whole.** IsOS `0.1.0` is the owner of the saved deployment graph, exact packages, generated configuration, local workbench, and direct EOS/EGIA component children. EOS `0.2.0` owns hardware admission, the Rust runtime and its MuJoCo child. EGIA `0.1.0` owns agent stages. IsosUI `0.1.0` edits drafts through IsOS and presents status. Model-like services remain separate, explicitly started IsOS services. Current commands and behavior are described in [first use](../usage/get-started.md); target data contracts live in [graph](../interfaces/node-graph.md), [local API](../interfaces/local-api.md), [packages](../interfaces/releases.md), and [configuration/logs](../interfaces/configuration.md).

```text
registry/catalog → verified release receipt → node types, sets, assets
                                           ↓
user draft → IsosUI/CLI → IsOS validate → revision-checked Apply → saved graph
                           ↓                   ↓
                    resolved selectors     immutable core config
                                               ↓
                          IsOS manager → EOS Rust → MuJoCo child/socket
                                       → EGIA Python stages
                          status/log cursors → IsosUI work panels
```

The graph is `core → layer → group → node`; a registered type fixes placement, capabilities, settings, typed ports, lifecycle and description. Instances fix identity, operator description, package pin and values. A package may contribute nodes without gaining authority to run arbitrary commands. IsOS owns verification and configuration; EOS owns an **internal adapter registry** for compiled implementations such as MuJoCo. The simulator executable and scene remain an external, exact-version IsOS package. EOS reads the applied graph and generated snapshot but never installs packages or edits the graph. One EOS dispatcher admits every actuator command, including those sent through package adapters.

## Admission and Apply algorithm

1. Read graph bytes and calculate SHA-256 revision before any normalization. Select the parser from the graph version. For the target reader, accept newly authored `0.1.0` and existing safe `1.0.0` graphs; reject unknown future versions. Normalize omitted optional descriptions and edge defaults **in memory only**.
2. Ask each enabled core to validate its subtree and type IDs. Merge built-in types with verified package sidecars; reject duplicate IDs, unregistered placement, incompatible typed ports and unsupported codecs. Preserve existing `@v1.*` IDs where registered. Never execute legacy arbitrary `command` nodes.
3. For every exact package pin, verify receipt, release manifest, executable, descriptor, scene and referenced assets. Resolve selector IDs to verified assets; an unavailable selected ID fails Apply with node/setting path. Validate simulator device IDs, socket ownership and adapter compatibility. The graph stores stable IDs and pins, not absolute asset paths.
4. Compare the caller's expected revision with the saved revision. For a new `0.1.0` draft, serialize canonical bytes and calculate their new SHA-256; for an unchanged legacy graph, retain its original bytes and revision. Render bounded EOS/EGIA setup files from those bytes, write private temporary files, and atomically publish a snapshot under the **new** revision. Check the expected saved revision again before atomically saving new draft bytes. An unused snapshot from a losing concurrent edit is safe to discard. Return the resulting revision; active children retain their previous revision until explicit Restart.

The explicit migration operation previews old-to-new graph and type changes, requires the original revision, and returns a new revision. It preserves a legacy graph's bytes until accepted. A failed parse, package check, render, or revision comparison leaves the saved graph and active processes unchanged.

## Run, stop and recovery algorithm

Run re-reads the saved bytes, package receipts, selected assets, configured core executables and generated snapshot identity. It validates version support against each core's advertised arrays. IsOS starts direct component children in dependency order and waits for readiness; EOS starts its MuJoCo child, connects its owner-only Unix socket and constructs declared adapters. EGIA starts only after its required EOS gateway capabilities are healthy. TCP listening alone is insufficient as EGIA readiness. On failure, IsOS stops dependents before providers, reports the last error and emits one state-change event. Stop tells EOS to stop admitted actuators before EOS terminates MuJoCo; IsOS then stops component process groups in reverse dependency order with bounded TERM/KILL grace. Restart failure restores previous specifications when possible and otherwise marks the deployment degraded.

A linked package may change on disk, so its fingerprint is checked at Apply and Run. An active process never hot-swaps binaries or scene bytes. Removal rejects a package pinned by saved or active deployment. Independent services have their own explicit lifecycle and never become hidden graph dependencies.

## Selector and wire algorithms

For each registered `Setting.selector`, identify its provider (`package`, `registry`, or `graph`), then derive options for the **draft** graph. Package provider reads only the verified exact pin. The result is grouped by node path and setting key; it includes stable IDs, labels, descriptions, availability and reasons. Preserve an unavailable selected value visibly so the operator can repair it. Apply re-resolves all selections. IsosUI's searchable control resembles a model-weight chooser but stores a stable ID, not a file path.

IsOS validates each edge's source, destination type, `priority` and `frequency_hz`. EOS owns routed delivery: a monotonic per-edge token bucket caps updates; weighted fair service and bounded queues prevent high-priority streams from starving others; overflow is counted and logged at a bounded rate. `expected_rate_hz` is producer metadata and never silently becomes an edge cap. A design-only edge is visible in IsosUI but has no claimed runtime scheduler. Priority cannot bypass dispatcher safety or actuator stop.

## Implementation milestones

The ordered dependencies, outputs and acceptance gates are maintained in the [single IsOS milestone plan](../maintenance/roadmap.md). This design page owns the algorithms above; contract fields stay in `interfaces/` and node behavior is indexed in the [node catalog](../layers/README.md). No WorkflowService, ActionList or workflow DAG is part of this release.
