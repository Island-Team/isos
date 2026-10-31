# IsOS 集成计划

[English](../../../docs/isos/maintenance/plan.md)

本文记录当前 IsOS 边界。[v2 图](../interfaces/node-graph-v2_cn.md)、[服务契约](../interfaces/services_cn.md)、[发行清单](../interfaces/releases_cn.md)和[本地 API](../interfaces/local-api_cn.md)定义正在使用的接口。

## 当前职责

| 所属项目 | 当前职责 |
| --- | --- |
| IsOS | 已配置核心路径和精确软件包收据、共享 v2 图准入、仅所有者可访问的管理器、直接持有 EOS/bridge/config 子进程、静态 IsosUI、显式启动的独立服务和日志 |
| EOS | 运行时、调度器与硬件准入、直接持有 MuJoCo 仿真器子进程 |
| EGIA | 图设计中的 common 节点；运行控制留待后续 |
| Island-models | 目录、预检和按别名托管启动器；IsOS 可适配显式服务文件 |
| Island Port | 经审核的发行元数据与账户认证 |

`isos init` 创建配置和通用图后退出。`isos serve` 运行 API、已配置界面包和管理器；`isos run` 启动已应用图并保持前台。`isos stop` 关闭所有 IsOS 所属工作；`isos deployment stop` 只停止图部署。服务使用图外的版本化 TOML 文件，且只能通过 `isos services start <id>` 显式启动。产品模式使用已配置且经验证的产物，不依赖源码构建或 npm。

EOS、EGIA 和 IsosUI 各自负责构建。IsOS 为每个核心使用一个已配置程序目录，并使用一个静态界面包；v2 构建和按提交选择命令已退出流程。`isos version check` 报告每个核心的产品版本和启动就绪状态。Apply 按已保存图修订生成核心配置；Run 复查修订、软件包和程序。MuJoCo 是外部 IsOS 软件包：单独安装或链接软件包不会激活图修改。

## 待完成工作

在各支持目标上验证真实 MuJoCo 与 Island-models 部署；发布批准的发行资产；建立跨仓库 API 与服务契约的标签兼容性；完成 EGIA 运行时集成。以后添加有类型的图日志节点和版本化 IPC 日志契约；目前仅所有者可访问的 `log_append` 操作用于提交选定事件。未来的图设计可以表示服务，但当前服务管理器须独立于 EOS 和 EGIA 节点依赖。保留旧版组件命令作为兼容入口，直到其调用方迁移完成。
