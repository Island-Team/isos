# IsOS 管理的后端软件包

[English](../../../docs/isos/design/backend-v2.md)

MuJoCo 是首个 Aether 后端包：`island/mujoco-sim@2.0.0`。包内含 `backend-v2.json`、`bin/eos-mujoco-sim`、场景和资源。发行清单描述分发与归档校验；后端描述文件声明可执行文件、场景 ID、设备 ID、协议和运行依赖；共享 `2.0.0` 图配置 `isos.process.backend@v1.0` 节点。三者职责不同，安装软件包不会暗中修改部署。

IsOS 在校验、应用和运行时解析精确固定版本。每次解析都会重新读取本地链接；检查描述文件身份、执行权限、所选场景、场景引用资源、MuJoCo 库、规范套接字路径，以及 EOS 适配器的设备和套接字引用。描述文件、可执行文件、场景和资源的指纹能发现运行中链接的变化。用户在图中选择场景并明确应用。EOS 只读取已校验的图，创建有类型的相机、底盘和机械臂适配器，通过 Unix 套接字连接仿真器；它不能安装软件包或启动仿真器。

套接字位于仅所有者可访问的 IsOS 运行目录下。IsOS 先启动后端，再启动 EOS；进程依赖图使 EOS 配置服务和运行时依赖后端。停止或提供者故障时先停止 EOS 依赖者。管理器持有直接子进程、就绪探针、有界重启和进程日志。任何已应用图仍引用精确软件包 ID 和版本时禁止删除。后续后端可复用同一进程边界；执行器类型仍需通过 EOS 安全准入。

`isos init` 仅首次写入起始图。Home 展示已安装或链接的包、场景与先决条件问题、保存及运行中修订、运行状态和日志。Graph 编辑唯一的 IsOS/EOS/EGIA 共享图。EGIA common 节点目前仅用于设计。可视布局与运行配置分开保存。

## 开发检查

用新的 `ISOS_HOME`，执行 `isos pkg link island/mujoco-sim 2.0.0 <bundle>`，再依次执行 `isos config init`、`isos config validate <graph>`、`isos config apply <graph>`、`isos run`、`isos restart`、`isos logs simulator` 和 `isos stop`。`MUJOCO_ROOT` 须指向已安装的 MuJoCo 运行环境，EOS 的 `aether-profile` 验证器和 Rust 可执行文件须可用。参见[节点字段](../interfaces/node-graph-v2_cn.md)和[软件包命令](../usage/packages_cn.md)。
