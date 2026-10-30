# IsOS guide

IsOS is the local owner of the Aether v2 graph, backend packages, workbench API, and direct-child process manager. The first complete target is `island/mujoco-sim@2.0.0`. [Start here](../../README.md), then read the [backend design](design/backend-v2.md), [graph fields](interfaces/node-graph-v2.md), [package workflow](usage/packages.md), [local API](interfaces/local-api.md), and [development checks](develop/backend-v2.md).

The old component graph and systemd commands remain as a side chain while their remaining callers are refactored. The v2 main chain uses one `2.0.0` graph and current `v1.0` node types. IsOS starts MuJoCo, EOS reads the graph and connects to it, and EGIA common nodes remain design-only.

[中文](../../docs_cn/isos/README_cn.md)
