# IsOS guide

IsOS is the local owner of the Aether v2 graph, backend packages, workbench API, and direct-child process manager. It also manages explicitly started model-like services outside the graph. The first complete backend target is `island/mujoco-sim@2.0.0`. [Start here](../../README.md), then read the [quick start](usage/get-started.md), [backend design](design/backend-v2.md), [graph fields](interfaces/node-graph-v2.md), [service contract](interfaces/services.md), [package workflow](usage/packages.md), [local API](interfaces/local-api.md), [source layout](develop/source-layout.md), and [development checks](develop/backend-v2.md).

The old component graph and systemd commands remain as compatibility paths; an optional user unit can run product `isos serve`. The v2 main chain uses one `2.0.0` graph and current `v1.0` node types. EOS starts MuJoCo from the package paths verified by IsOS and connects its adapters to it; EGIA common nodes remain design-only.

[中文](../../docs_cn/isos/README_cn.md)
