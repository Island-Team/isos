# R06 IsOS 依赖停止用例

[English](README.md) · [归属方契约](../../../docs_cn/isos/interfaces/dependent-stop_cn.md) · [设计](../../../docs_cn/isos/design/dependent-stop_cn.md)

**仅设计用例。** `cases.json` 冻结 22 个桩和真实归属方实现测试的预期结果。`python3 tests/fixtures/dependent_stop/check_cases.py` 检查形状与安全关键预期；它不运行进程，也不能证明实体停止。EOS 归属方确定精确 R06 契约后，实现必须把 EOS 终结收据绑定到选中子进程、运行 ID 和观测 PID。用例中的 `eos_receipt` 标记是测试场景，不是 EOS wire 字段。

使用一个干净的 `ISOS_HOME` 重放正反用例：直接 CLI、HTTP 和归属方 socket Stop、附着 Ctrl+C、Restart／回滚、提供者崩溃、管理器 Shutdown、事件轮转和故障。对比前后的进程组成员、真实 EOS/MuJoCo 子进程及 socket，并比较运行 ID 与授权，但不打印授权值。完整输出保存在忽略的 `logs/test/<run-id>/`。停止未知时必须阻止新候选，不能因为 PID 消失就写 `deployment_stopped`。最终真实归属方重放需要 EOS 和固定 MuJoCo `0.1.1` 软件包的两个场景；桩可测试顺序，但不能关闭 R06 门槛。
