# IsOS node catalog

[中文](../../../docs_cn/isos/layers/README_cn.md) · [Graph contract](../interfaces/node-graph.md) · [First use](../usage/get-started.md)

This is the **navigation and behavior catalog** for IsOS-owned node kinds. The path reflects the graph's `core → layer → group → node` hierarchy. The [graph contract](../interfaces/node-graph.md) owns serialized fields and compatibility; [configuration](../interfaces/configuration.md) owns generated files and logs. A leaf page describes what an operator sees, what starts, and how failure or stop behaves. New `@v0.1` entries are target design; existing `@v1.*` IDs retain their established behavior.

| Core | Layer | Group | Node kind | Runtime status |
| --- | --- | --- | --- | --- |
| `isos` | `process` | `component` | [Managed component](process/groups/component/nodes/managed-component.md) | Current `@v1.1`; target `@v0.1` |
| `isos` | `ui` | `panel` | [Vision panel](ui/groups/panel/nodes/vision.md) | Current `@v1.0`; target catalog description |

IsOS also stores EOS and EGIA subtrees without taking ownership of their node behavior. The EOS dispatcher, controls, hardware, and internal MuJoCo adapter belong in the [Island-eos](https://github.com/Island-Team/Island-eos) (`docs/eos/layers/`); EGIA stage kinds belong in the [Island-egia](https://github.com/Island-Team/Island-egia) (`docs/egia/layers/`). A package contribution may add registered kinds under its declared owner; it cannot gain arbitrary process or actuator authority. Independent model-like services use the [service contract](../interfaces/services.md) and are not nodes in this release. There is no workflow node kind in the 0.x target.
