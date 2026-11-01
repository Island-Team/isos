# `isos / ui / panel / vision`

[English](../../../../../../../docs/isos/layers/ui/groups/panel/nodes/vision.md) · [图契约](../../../../../interfaces/node-graph_cn.md)

**类型：** 当前 `isos.ui.vision@v1.0`；新的目录注册使用带非空类型说明的 `@v0.1`。**类型说明：** 为已接纳的相机来源声明工作台 Vision 视图。它是 UI 投影，不是 IsOS 子进程、相机驱动或执行器路由。

**输入与输出：** 输入 `preview: VisionFrame` 来自已注册 EOS camera-gate 输出，经 Vision WebSocket 路由交付。视图向 IsosUI 展示来源身份、帧状态和错误，没有数据输出。可选设置为 `target_fps`（正整数，最多 60 Hz）、`max_width_px`（正整数，最多 4096 px）和 `jpeg_quality`（整数 1–100）。默认值及帧语义由 EOS Vision 契约定义。图保存 gate 引用，不保存帧缓冲或凭据；不可用 gate 保留可见及原因，不暗中切换。

**生命周期：** 部署就绪后可挂载面板。相机缺失或停止时显示不可用状态和最近错误，不使部署保持运行，也不偷偷启动模拟器。关闭或浮动面板只改变 UI 布局。停止时取消订阅并释放帧缓冲。

**兼容：** 已有 `@v1.0` 实例保留既有投影。新增可选展示字段可使用默认值；改变相机身份、端口类型或帧语义须注册新类型。布局持久化由 IsosUI 工作区负责。
