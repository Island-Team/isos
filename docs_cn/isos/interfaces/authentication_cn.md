# Island-port 身份验证

`isos login` 使用 GitHub OAuth Device Flow 登录，并将临时 GitHub 用户令牌交换为 Island-port 访问令牌和刷新令牌。命令仅申请 GitHub `user:email` 权限，不会将 GitHub 令牌写入磁盘。IsOS 将 Island-port 凭据保存到配置目录中的 `auth.json`，并设置为仅当前用户可读写。

必须在 IslandOS OAuth 应用中启用 GitHub Device Flow，并在 Island-port 服务中设置 `ISLAND_GITHUB_DEVICE_FLOW_ENABLED=true`。公共接口 `GET /api/auth/github/config` 通过 `device_flow_enabled` 字段报告此状态。任一设置未启用时，`isos login` 会说明登录不可用。

    isos login
    isos whoami
    isos packages publish --manifest island-release.json
    isos logout

GitHub 登录会按已验证邮箱关联现有账号并保留其角色。新建的 GitHub 账号为 member，可以提交软件包等待审核；目录直发布需要 developer 角色。发布命令将完整且已校验的 schema v4 清单提交给 Island-port；对应压缩包须预先上传到匹配的 GitHub Release。IsOS 不会申请或保存 GitHub 仓库写入凭据。

Island-port API 返回 401 时，IsOS 会刷新访问凭据、轮换刷新令牌并保存新令牌。退出登录会撤销刷新会话并删除本地凭据文件。访问令牌有效期较短，刷新令牌有效期较长；文件权限限制为当前用户访问。

相关文档：[软件包操作](../usage/packages_cn.md)、[发行清单](releases_cn.md)、[Island-port 身份验证 API](https://github.com/Island-Team/Island-port/blob/master/docs/api/auth.md)。
