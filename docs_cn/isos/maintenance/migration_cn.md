# 从旧启动器迁移

旧版 EOS island-launch、island-pkg 和 EGIA bin/island 在各自源码仓库保留兼容入口。新部署的进程组合由 IsOS 管理。

1. 安装 IsOS 并运行 isos doctor。
2. 执行 isos config init，备份旧 stack.toml、eos.toml、island.toml 和模型服务配置。
3. 在 JSON 中填写 EOS/EGIA 命令，将模型进程设为 service 节点、硬件或仿真进程设为 backend 节点，并连接依赖边。
4. 使用 isos packages install 或 link 选择准确的版本，在节点中记录软件包 ID 和版本。
5. 执行 isos config validate <file> 和 isos config apply <file>。
6. 停止旧启动器，再执行 isos start、isos status 和 isos eos enter。

当前版本尚未自动导入旧 TOML，也不会移动模型权重。生成的 stack.toml 是旧 EOS 工具的适配文件；请编辑 JSON 部署图，而不是修改生成结果。不要让旧 EGIA 组合启动器与 IsOS 同时管理同一部署。

[English](../../../docs/isos/maintenance/migration.md)

## 配置图 v1 升级到 v2

保存本地编辑后运行 `isos config migrate`。IsOS 创建带时间戳的 `.bak` 副本，加入 EOS/EGIA 精确配置引用、逻辑 `eos.uds` 端点和 EGIA 类型连接，在替换原图前校验结果。严格 JSON 解码器拒绝未知字段，不会静默丢弃。运行前需选择组件构建并应用迁移后的配置图。
