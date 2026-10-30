# Release manifest

[中文](../../../docs_cn/isos/interfaces/releases_cn.md)

The approved catalog at https://www.os-island.com is the default source of release metadata. GitHub Releases are the default asset host. Installation does not activate a package. A deployment graph selects exact package versions, and config apply makes the selection explicit.

After attaching the verified release archive to its GitHub Release, a package developer can publish the complete v4 manifest with `isos packages publish --manifest island-release.json`. The CLI submits metadata to Island-port; it does not upload GitHub release assets. See [IsOS authentication](authentication.md) and [package operations](../usage/packages.md).

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

## Package node contributions

The `1.0.1` asset snippet above illustrates the release envelope and is historical. A v2 MuJoCo node package uses `island/mujoco-sim@2.0.0` and the contribution fields below.

Any v4 package kind may declare `node_contributions`. `node_types` and `safety_adapters` use EOS v1.0 registry JSON. `data_types` declares package-specific data IDL entries. IsOS snapshots these arrays with the exact `{id,version}` package pin into `node-types.json` beside the receipt; EOS loads that sidecar through repeatable `--node-types` options. Each data type requires `id` (versioned `namespace.name@vM.m` string), `version` (`M.m` string matching the ID suffix), `purpose` (nonempty human-readable string), `media_type` (nonempty MIME string), `codec` (nonempty registered runtime codec ID, such as `json_v1`), `schema` (JSON Schema object), and `max_payload_bytes` (positive integer bytes). The EOS runtime admits a custom data type only when its codec is compiled and registered. Contributed actuator types need a registered safety adapter with stop support. A linked bundle uses `isos pkg link ID VERSION BUNDLE --manifest release.json`; IsOS validates and snapshots the full v4 manifest. Old links without a manifest remain valid but contribute no node sets.

`node_sets` contains objects with `id`, optional `parameters`, and `nodes`. Each node declares a symbolic `ref`, `core`, `layer`, optional `group`, and complete `profile`. The default group is the package name after `/`; MuJoCo declares `mujoco` explicitly. A parameter has `{ "type": "string", "default": "optional" }`; `${name}` in any profile string is replaced by the provided value. A profile reference to `@ref` resolves to the corresponding full graph path, including in `references`, `inputs`, and adapter settings. IsOS injects `profile.package={id,version}` only for node types contributed by that package and rejects conflicting pins. Built-in EOS and IsosUI nodes in a package set remain unpinned. Profile fields follow the [EOS node graph contract](node-graph-v2.md).

Package installation only makes node sets available. `isos pkg node-sets` lists them, and `isos pkg preview-set ID VERSION SET_ID graph.json --values values.json` prints a composed graph and `added_paths`. The HTTP preview has the same behavior. For a saved legacy MuJoCo graph, `--replace-legacy-mujoco` or preview body `replace_legacy_mujoco:true` removes the old backend, hardware, and dependent control nodes in the returned draft before adding the new set. Applying still uses the revision-checked graph API. Removal rejects any saved graph node with the exact package pin.

Package `node_contributions.adapter_families` is an optional array of adapter implementation descriptors. Each descriptor requires `id` (nonempty stable string, unique within a release), `device_kinds` (nonempty array of device-class strings such as `camera`, `base`, `manipulator`), `transport` (nonempty transport string such as `unix_socket`), and `description` (nonempty human-readable purpose). It may include `supported_primitives` (array of exact `name@version` strings) and `settings_schema` (JSON Schema object for adapter settings). IsOS preserves package provenance in the verified sidecar. EOS computes `compiled` and an unavailable `reason` in its catalog; a package cannot claim that its adapter is compiled. Generic hardware node types remain independent of adapter identity.
