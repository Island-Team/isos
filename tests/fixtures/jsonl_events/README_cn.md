# P06 统一 JSONL 事件夹具

[English](README.md) · [归属契约](../../../docs_cn/isos/interfaces/events_cn.md) · [设计](../../../docs_cn/isos/design/events_cn.md)

`cases.json` 冻结 30 个成功、拒绝、抑制和生命周期场景，案例名称供实现测试稳定引用。检查器验证案例目录及代表性原始子进程／存储记录边界，包括 8,064/8,065 与 8,192/8,193 字节。执行 `python3 tests/fixtures/jsonl_events/check_cases.py`。这不代表运行时收集器已实现。

实现时对 `LogStore`、`Supervisor`、管理器状态和所选 stub 子进程逐案测试。原始超长或规范化后超长记录须增加有界 `log_ingest_rejected` 原因计数，不得复制原行；丢弃原始超长行后仍须读取下一条有效行。把所选 `eos`、`bridge`、`config`、`egia` 来源绑定至直接子进程角色，比对活动 `run_id`，拒绝原始 `pid` 和节点成员指针。测试管理器会话下的启动前失败、精确软件包 pin 与已验证收据来源、候选／恢复运行 ID、抑制、轮转和不阻塞停止。最终归属边界测试回放真实 EOS 与 EGIA 的 stderr。
