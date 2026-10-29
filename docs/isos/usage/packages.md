# Work with packages

IsOS discovers approved packages through os-island.com and downloads matching assets from GitHub Releases. Installation verifies the selected platform, byte size, and SHA-256. It does not activate a package; select its exact ID and version in the [deployment graph](../interfaces/deployment-graph.md). Omit the search term to list the latest approved releases.

    isos packages search mujoco
    isos packages info island/mujoco-sim --version 1.0.1
    isos packages install island/mujoco-sim --version 1.0.1
    isos packages list

Package developers can sign in with the IslandOS GitHub OAuth app and publish a manifest for a release whose assets are already attached to GitHub Releases:

    isos login
    isos whoami
    isos packages publish --manifest island-release.json

Direct catalog publication requires an Island-port `developer` role. `isos logout` revokes the saved Island-port refresh session. `isos login` stores Island-port credentials in the IsOS config directory with owner-only permissions; it exchanges the temporary GitHub token immediately and does not save it. The GitHub OAuth app and Island-port must have CLI Device Flow enabled; see [authentication](../interfaces/authentication.md).

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
