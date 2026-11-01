# R03 IsOS 控制授权交接用例

[English](README.md) · [IsOS 所有者契约](../../../docs_cn/isos/interfaces/control-grants_cn.md) · [设计](../../../docs_cn/isos/design/control-grants_cn.md)

`cases.json` 固定 IsOS 签发和启动传递的设计矩阵，尚不表示运行时已通过。基线使用干净的 `ISOS_HOME`、已验证的 schema-2 图生成代、已选 EOS stub、可选已选 EGIA stub 和 EOS bridge stub。stub 只记录看到的保留环境变量**名称**。EOS R03 测试独立检查验证文件字节、gRPC 元数据、角色/方法范围及 Dispatcher 释放。任何用例都不把真实授权写入源码仓库。

正例覆盖精确文件格式/权限、三个接收方路径、与保存修订及运行 ID 绑定的验证文件、幂等 Start 和自动子进程重启的重用、相同修订显式 Restart 的轮换，以及替换失败后使用另一个新运行恢复。反例注入不安全文件、错误授权/验证格式、修订变化、已选接收方缺失、P04 快照缺失、旧 `1.0.0` 图和测试用随机源故障。替换前拒绝必须保持活动 PID、修订与授权包不变。替换失败必须使候选运行终止；旧凭据不能授权恢复。Stop 在回收接收方后撤销授权包。状态、API 错误和当前运行 JSONL 事件只能含脱敏代码、运行 ID 与修订，不得包含授权、其摘要或 bearer 文本。

只给所选 profile 或只给验证文件的半托管 EOS 启动必须在监听器绑定前失败。两者皆无时即使存在 `--run-id`，仍为独立只读。

实现测试应将每个 `id` 映射到单元或干净 home 的边界测试，在忽略的 `logs/test/<run-id>/` 记录命令、所选 EOS/IsOS 提交与结果，然后获得独立审查。运行 `python3 tests/fixtures/control_grants/check_cases.py` 验证设计矩阵与双语链接。
