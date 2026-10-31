# IsOS guide

IsOS is the local owner of the Aether v1 graph, backend packages, workbench API, and direct-child process manager. It also manages explicitly started model-like services outside the graph. The first complete backend target is `island/mujoco-sim@2.0.0`. [Start here](../../README.md), then read the [quick start](usage/get-started.md), [backend design](design/backend-v2.md), [graph fields](interfaces/node-graph-v1.md), [service contract](interfaces/services.md), [package workflow](usage/packages.md), [local API](interfaces/local-api.md), [source layout](develop/source-layout.md), and [development checks](develop/backend-v2.md).

The saved `.nodes.json` graph is the only deployment path. An optional user unit can run product `isos serve`. The graph contract is `1.0.0`; node type versions are independent. EOS starts MuJoCo from the package paths verified by IsOS and connects its adapters to it; EGIA common nodes remain design-only.

[中文](../../docs_cn/isos/README_cn.md)
