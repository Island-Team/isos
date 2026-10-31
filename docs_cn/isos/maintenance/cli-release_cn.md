# 发布 IsOS CLI

[English](../../../docs/isos/maintenance/cli-release.md)

手动的 `IsOS CLI release` 工作流打包 Linux x86_64 CLI 和本地 API 服务，不包含 IsosUI。公开的 IsosUI 仓库单独发布静态页面包；Island-web 另行提升托管版本。

1. 将预定的 IsOS 提交合并到 `master` 并完成验证。确认 `Cargo.toml` 中的版本号正确，且 `v<版本>` 标签尚不存在。
2. 在 `master` 上以 `phase=prepare` 运行工作流。它检查格式、Clippy 和测试，使用锁定依赖构建发行版，然后上传临时压缩包、`SHA256SUMS` 和源码提交元数据。查看运行结果，并下载检查构建产物。
3. 在 `master` 上以 `phase=publish` 再次运行工作流，填入成功的 prepare 运行 ID。它校验产物版本、源码提交祖先关系和 SHA-256，然后创建 GitHub Release 与标签。批准准备好的压缩包之前，不要执行 publish。

发行压缩包包含 `isos`、`LICENSE` 和 `README.md`。将二进制文件安装到 `PATH` 中的目录；`./install.sh` 只用于源码仓库，不在发行压缩包内。在 IsosUI 自身仓库构建，并通过 `paths.ui_dist` 或 `ISOSUI_DIST_DIR` 配置生成且经校验的文件包，即可无需 npm 提供页面。分别配置 EOS 和 EGIA 程序目录；`isos version check` 报告就绪情况。
