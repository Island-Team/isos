# Work with packages

IsOS discovers approved packages through os-island.com and downloads matching assets from GitHub Releases. Installation verifies the selected platform, byte size, and SHA-256. It does not activate a package; select its exact ID and version in the [deployment graph](../interfaces/deployment-graph.md). Omit the search term to list the latest approved releases.

    isos packages search mujoco
    isos packages info island/mujoco-sim --version 1.0.1
    isos packages install island/mujoco-sim --version 1.0.1
    isos packages list

Package developers can sign in with the IslandOS GitHub OAuth app and publish a manifest for a release whose assets are already attached to GitHub Releases:

    isos auth login
    isos auth whoami
    isos packages publish --manifest island-release.json

Direct catalog publication requires an Island-port `developer` role. `isos auth login` stores Island-port credentials locally with owner-only permissions; it exchanges the temporary GitHub token immediately and never saves it. `isos auth logout` revokes the saved refresh session and removes the local credentials. The GitHub OAuth app and Island-port must have CLI Device Flow enabled; see [authentication](../interfaces/authentication.md).

For a development checkout, link an exact version:

    isos packages link island/eos 2.0.0 /path/to/Island-eos

For a local candidate release, verify the archive and explicitly opt into its unapproved manifest:

    isos packages verify island-release.json asset.tar.gz
    isos packages install --manifest island-release.json --asset asset.tar.gz --allow-unapproved

See the [release manifest contract](../interfaces/releases.md) for required metadata and activation templates.

Use the scoped library commands to inspect deployed nodes and install releases of the matching type:

    isos services list
    isos services info vision
    isos services status vision --json
    isos backends install island/mujoco-sim --version 1.0.1
    isos skills install island/example-skill --version 1.0.0

The install command checks the release kind before downloading. It leaves graph activation to isos config apply. Each library also has remove <id> <version>, which rejects packages referenced by a selected or active deployment.

[中文](../../../docs_cn/isos/usage/packages_cn.md)

## Upload a v4 release

`isos pkg upload <manifest.json> <asset.tar.gz>` checks the manifest, platform asset size and SHA-256, exact existing GitHub tag, and draft Release asset names. It uses the authenticated `gh` CLI to create or reuse a draft GitHub Release, uploads without overwriting an existing asset, and submits the manifest to Island Port using `isos auth login`. A submission under review is reported separately from a published release. `isos packages publish --manifest <file>` remains the metadata-only compatibility path.
