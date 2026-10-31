# IsOS 集成计划

[English](../../../docs/isos/maintenance/plan.md)

本文记录当前 IsOS 边界。[v2 图](../interfaces/node-graph_cn.md)、[服务契约](../interfaces/services_cn.md)、[发行清单](../interfaces/releases_cn.md)和[本地 API](../interfaces/local-api_cn.md)定义正在使用的接口。

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

[0.x 架构 TODO](../design/system_cn.md#按依赖顺序的实现-todo)按依赖顺序列出图兼容、包选择器、配置／版本／日志契约、EOS–EGIA–MuJoCo 集成、UI、清理和发行验证。现有仅所有者可访问的 `log_append` 是提交选定事件的小型本地接口；目标统一事件通过可选字段扩展，旧调用方仍有效。发布获批资产前须在支持目标上验证真实 MuJoCo 和 Island-models 部署。服务保持独立，不成为 EOS／EGIA 图的隐式依赖。
