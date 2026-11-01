# `isos / ui / panel / vision`

[中文](../../../../../../../docs_cn/isos/layers/ui/groups/panel/nodes/vision_cn.md) · [Graph contract](../../../../../interfaces/node-graph.md)

**Type:** current `isos.ui.vision@v1.0`; a new catalog registration uses `@v0.1` with a nonblank type description. **Description:** Declares a Vision view in the workbench for an admitted camera source. It is a UI projection, not an IsOS child process, camera driver, or actuator route.

**Inputs and outputs:** Input `preview: VisionFrame` comes from a registered EOS camera-gate output over the Vision WebSocket route. The view exposes source identity, frame status and errors to IsosUI; it has no data output. Optional settings are `target_fps` (positive integer, at most 60 Hz), `max_width_px` (positive integer, at most 4096 px), and `jpeg_quality` (integer 1–100). The EOS Vision contract owns their defaults and frame semantics. The graph stores the gate reference, not frame buffers or credentials. An unavailable gate remains visible with a reason rather than silently switching.

**Lifecycle:** The panel may mount after the deployment becomes ready. Missing or stopped camera data shows unavailable status and a last-error code; it does not keep the deployment alive or create a hidden simulator process. Closing or floating the panel changes only UI layout. Stop cancels subscriptions and releases frame buffers.

**Compatibility:** Existing `@v1.0` instances remain readable for their current projection. A new optional presentation field can use a default; changing camera identity, port type, or frame semantics requires a new type registration. The IsosUI workspace owns layout persistence, not this graph node.
