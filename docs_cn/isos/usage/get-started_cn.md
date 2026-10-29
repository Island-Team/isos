# 启动 IsOS 部署

[English](../../../docs/isos/usage/get-started.md)

在具有 systemd 用户会话和 Rust 1.98 或更新版本的 Linux 上，构建并安装 IsOS 与独立 Island-web 页面包。发行构建可用 `ISOS_WEB_BUNDLE` 指定页面包，本地构建使用相邻的 Island-web 仓库。

    ./build.sh
    ./install.sh
    isos init
    isos open

从源码构建时，将 `EOS_REPO_PATH` 和 `EGIA_REPO_PATH` 指向已提交的仓库。仓库命令包装器会读取 `.env`。IsOS 创建明确的本地构建候选；除非传入 `--install-deps`，EOS 构建不会安装系统依赖。

    isos build eos
    isos build egia
    isos version pin eos <精确提交>
    isos version pin egia <精确提交>
    isos version check

在编辑器中启用 EOS、逻辑 UDS 端点和 EGIA。校验与应用是独立操作；过期的编辑版本会返回冲突。两个运行时停止时仍可编辑配置。

    isos run
    isos status
    isos logs egia
    isos stop

`run` 按依赖顺序启动节点，检查已选构建摘要及 EOS/EGIA 协议。EGIA 直接连接 EOS 的私有 UDS。停止时按反向顺序关闭。更新运行中部署时，先应用配置图，再明确重启。EOS 自带演示绑定无需仿真服务即可启动，但演示动作需仿真服务运行后才可使用。
