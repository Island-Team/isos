# Island-port authentication

`isos login` signs in through GitHub OAuth Device Flow and exchanges the temporary GitHub user token for an Island-port access and refresh token. It requests only the GitHub `user:email` scope. The GitHub token is never written to disk. IsOS stores Island-port credentials in `auth.json` under its config directory using owner-only permissions.

GitHub Device Flow must be enabled in the IslandOS OAuth app. Island-port must be deployed with `ISLAND_GITHUB_DEVICE_FLOW_ENABLED=true`; the public `GET /api/auth/github/config` response advertises this as `device_flow_enabled`. If either setting is disabled, `isos login` reports that login is unavailable.

    isos login
    isos whoami
    isos packages publish --manifest island-release.json
    isos logout

GitHub sign-in preserves the role of an existing account matched by verified email. A new GitHub-linked account is a member and can submit packages for review, but direct publication requires the developer role. Package publication sends a complete validated schema v4 manifest to Island-port; the referenced archive must already be attached to the matching GitHub release. GitHub repository credentials are not requested or stored by IsOS.

IsOS refreshes Island-port access credentials after an API `401`, rotates the refresh token, and persists the replacement. Logout revokes the refresh session and removes the local credential file. Island-port issues a short-lived access token and a refresh token with a longer lifetime; file permissions restrict local access to the current user.

Related: [package operations](../usage/packages.md), [release manifest](releases.md), [Island-port authentication API](https://github.com/Island-Team/Island-port/blob/master/docs/api/auth.md).
