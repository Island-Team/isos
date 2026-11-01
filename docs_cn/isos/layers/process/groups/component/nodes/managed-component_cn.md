# `isos / process / component / managed-component`

[English](../../../../../../../docs/isos/layers/process/groups/component/nodes/managed-component.md) · [设置与图字段](../../../../../interfaces/node-graph_cn.md#组件进程节点)

**类型：** 当前 `isos.process.component@v1.1`；新注册的目标类型为 `isos.process.component@v0.1`。**类型说明：** 从已验证核心安装中启动一个 EOS 或 EGIA 组件程序，观察就绪并停止其进程组。实例 ID 是路径末段，实例说明描述部署角色。此节点不接受任意可执行程序。

**输入与输出：** `depends_on` 引用决定启动顺序；不暴露命令或物理数据端口。状态经[本地 API](../../../../../interfaces/local-api_cn.md)投影 PID、就绪、重启次数、最近错误和活动图修订。`role` 与有界 `args` 选择已安装程序；所有类型、限制和默认值以图契约为准。

**生命周期：** Apply 前校验角色、软件包与程序；Run 时先启动依赖，再启动直接子进程并探测就绪。依赖失败会阻止启动。就绪超时报告节点路径和错误，先停依赖方，再按有界重启策略恢复。停止时先停依赖方，再向进程组发送 TERM，等待两秒后 KILL 残留后代。PID 或 TCP 监听不足以证明 EGIA 已完成 EOS 发现。

**兼容：** 已有安全 `@v1.1` 实例在显式、带修订检查的迁移前保持原始字节和语义。旧 `@v1.0` 任意 `command` 设置只返回迁移指引，绝不执行。新增可选设置须有默认值；改变 `role`、就绪或停止语义须使用新的类型 ID。
