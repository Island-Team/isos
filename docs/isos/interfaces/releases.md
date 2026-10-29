# Release manifest

[中文](../../../docs_cn/isos/interfaces/releases_cn.md)

The approved catalog at https://www.os-island.com is the default source of release metadata. GitHub Releases are the default asset host. Installation does not activate a package. A deployment graph selects exact package versions, and config apply makes the selection explicit.

    isos packages search mujoco
    isos packages info island/mujoco-sim --version 1.0.1
    isos packages install island/mujoco-sim --version 1.0.1
    isos packages list

For a local development checkout:

    isos packages link island/eos 2.0.0 /path/to/Island-eos

For a candidate release not yet admitted to the catalog:

    isos packages verify island-release.json asset.tar.gz
    isos packages install --manifest island-release.json \
      --asset asset.tar.gz --allow-unapproved

An unapproved manifest requires the explicit flag. IsOS verifies platform, size, and SHA-256 before extracting. Archive entries must be regular files or directories with safe relative paths; links and traversal are rejected. Installed packages live under ~/.local/share/isos/packages/<namespace>/<name>/<version>/. The receipt records the selected release. A linked checkout uses a receipt that points to the existing directory.

## Release JSON version 4

The release file is named island-release.json and is formatted JSON.

    {
      "schema_version": 4,
      "id": "island/mujoco-sim",
      "kind": "backend",
      "version": "1.0.1",
      "source_commit": "0000000000000000000000000000000000000000",
      "source_url": "https://github.com/Island-Team/isos-mujoco-sim",
      "release_url": "https://github.com/Island-Team/isos-mujoco-sim/releases/tag/v1.0.1",
      "contracts": ["eos.simulator.v1"],
      "assets": [{
        "name": "mujoco-sim-linux-x86_64.tar.gz",
        "os": "linux",
        "arch": "x86_64",
        "libc": "glibc>=2.35",
        "url": "https://github.com/Island-Team/isos-mujoco-sim/releases/download/v1.0.1/mujoco-sim-linux-x86_64.tar.gz",
        "size_bytes": 123456,
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "required_libraries": ["libmujoco.so.3"],
        "tested_ubuntu": ["22.04"]
      }],
      "activation": {
        "files": [
          {"source": "config/module.toml", "destination": "module.toml"}
        ]
      }
    }

The example hashes and size are placeholders. Supported kinds are component, service, backend, skill, extension, and example. The resolver accepts optional description, license, source_url, and release_url fields; the approved catalog requires source and release URLs for review. Optional asset selectors include gpu and ros; name, required_libraries, and tested_ubuntu preserve diagnostic metadata. IsOS chooses an asset only when OS, architecture, declared runtime selectors, and minimum glibc match the host. The catalog records review and publication policy; IsOS validates its downloaded v4 manifest again before using it.

Activation files are TOML templates. IsOS replaces {{package_file:relative/path}} with an absolute path inside the installed package, validates the resulting TOML, and writes it under the generated deployment directory. A rendered module.toml is included in the generated stack. Activation never starts a process.

The current IsOS CLI does not import the older EOS isos.lock or v1-v3 manifests. Use the [manual migration guide](../maintenance/migration.md) until an explicit import command is implemented.

Related: [deployment graph](deployment-graph.md), [package operations](../usage/packages.md).
