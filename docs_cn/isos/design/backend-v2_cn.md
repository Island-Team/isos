# IsOS 管理的后端软件包

[English](../../../docs/isos/design/backend-v2.md)

MuJoCo 是首个 Aether 后端包：`island/mujoco-sim@2.0.0`。包内含 `backend-v2.json`、`bin/eos-mujoco-sim`、场景和资源。发行清单描述分发与归档校验；后端描述文件声明可执行文件、场景 ID、设备 ID、协议和运行依赖；共享 `2.0.0` 图配置 `eos.process.backend@v1.0` 节点。三者职责不同，安装软件包不会暗中修改部署。

IsOS 在校验、应用和运行时解析精确固定版本。每次解析都会重新读取本地链接；检查描述文件身份、执行权限、所选场景、场景引用资源、MuJoCo 库、规范套接字路径，以及 EOS 适配器的设备和套接字引用。描述文件、可执行文件、场景和资源的指纹能发现运行中链接的变化。用户在图中选择场景并明确应用。IsOS 启动 EOS 时传入已验证的可执行文件与场景绝对路径。EOS 读取图、启动 MuJoCo 子进程，并用有类型的相机、底盘和机械臂适配器连接套接字；EOS 不能安装软件包或修改已应用图。

套接字位于仅所有者可访问的 IsOS 运行目录下。IsOS 先启动 EOS；EOS 启动 MuJoCo 并等待套接字就绪，然后才允许硬件操作。IsOS 管理 EOS、HTTP 桥和配置服务的子进程，EOS 管理仿真器子进程。停止 EOS 会终止 MuJoCo；仿真器意外退出时 EOS 失败，由 IsOS 重启整个 EOS 组件。任何已应用图仍引用精确软件包 ID 和版本时禁止删除。后续后端可复用同一进程边界；执行器类型仍需通过 EOS 安全准入。

`isos init` 仅首次写入起始图。Home 展示已安装或链接的包、场景与先决条件问题、保存及运行中修订、运行状态和日志。Graph 编辑唯一的 IsOS/EOS/EGIA 共享图。EGIA common 节点目前仅用于设计。可视布局与运行配置分开保存。

## 开发检查

用新的 `ISOS_HOME`，执行 `isos pkg link island/mujoco-sim 2.0.0 <bundle>`，再依次执行 `isos config init`、`isos config validate <graph>`、`isos config apply <graph>`、`isos run`、`isos restart`、`isos logs eos` 和 `isos stop`。`MUJOCO_ROOT` 须指向已安装的 MuJoCo 运行环境，EOS 的 `aether-profile` 验证器和 Rust 可执行文件须可用。参见[节点字段](../interfaces/node-graph-v2_cn.md)和[软件包命令](../usage/packages_cn.md)。
