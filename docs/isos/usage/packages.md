# Aether backend packages

[中文](../../../docs_cn/isos/usage/packages_cn.md)

`island/mujoco-sim@0.1.1` is the first backend bundle. It contains the executable, scene choices, assets, a node set, and `backend-v2.json`. The v4 release manifest is distribution metadata, separate from the v1 graph and node types. Installing or linking a bundle never activates it. `isos init` creates a generic draft without hardware. Preview the installed node set, choose a scene, and Apply the resulting graph before Run.

For local development, build the MuJoCo repository's simulator and stage a bundle with `python3 tools/build-backend-bundle.py --binary <simulator> --output <new-directory>`. Then:

```sh
isos init
isos pkg link island/mujoco-sim 0.1.1 /absolute/path/to/bundle --manifest /absolute/path/to/bundle/release.json
isos pkg node-sets
# Preview sim-hardware in the workbench, or use isos pkg preview-set with a values file.
isos config validate ~/.config/isos/deployments/default.nodes.json
isos config apply ~/.config/isos/deployments/default.nodes.json
isos run
```

For a release archive, `isos pkg verify <manifest> <asset>` checks the platform, size, and SHA-256. `isos pkg install --manifest <manifest> --asset <asset> --allow-unapproved` installs a local candidate. An approved catalog version can be installed with `isos pkg install island/mujoco-sim --version 0.1.1` after publication. `isos pkg list` shows installed versions. Apply and Run revalidate linked content, executable, selected scene and referenced assets, device IDs, and `MUJOCO_ROOT`. `isos pkg remove island/mujoco-sim 0.1.1` rejects a version referenced by an applied or active graph.

See [backend design](../design/backend-v2.md) and [release metadata](../interfaces/releases.md).
