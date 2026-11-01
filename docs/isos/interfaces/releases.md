# Release manifest

[中文](../../../docs_cn/isos/interfaces/releases_cn.md)

The approved catalog at https://www.os-island.com is the default source of release metadata. GitHub Releases are the default asset host. Installation does not activate a package. A deployment graph selects exact package versions, and config apply makes the selection explicit.

The P01 local integrity and receipt boundary is specified in [verified package admission](package-verification.md). Historical v4 examples below describe the release envelope; verified `0.x` Apply and Run require a P01 receipt and revalidation of the exact pin.

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

Related: [deployment graph](node-graph.md), [package operations](../usage/packages.md).

## Package node contributions

The `1.0.1` asset snippet above illustrates the release envelope and is historical. A v2 MuJoCo node package uses `island/mujoco-sim@0.1.1` and the contribution fields below.

Any v4 package kind may declare `node_contributions`. `node_types` and `safety_adapters` use EOS v1.0 registry JSON. `data_types` declares package-specific data IDL entries. IsOS snapshots these arrays with the exact `{id,version}` package pin into `node-types.json` beside the receipt; EOS loads that sidecar through repeatable `--node-types` options. Each data type requires `id` (versioned `namespace.name@vM.m` string), `version` (`M.m` string matching the ID suffix), `purpose` (nonempty human-readable string), `media_type` (nonempty MIME string), `codec` (nonempty registered runtime codec ID, such as `json_v1`), `schema` (JSON Schema object), and `max_payload_bytes` (positive integer bytes). The EOS runtime admits a custom data type only when its codec is compiled and registered. Contributed actuator types need a registered safety adapter with stop support. A linked bundle uses `isos pkg link ID VERSION BUNDLE --manifest release.json`; IsOS validates and snapshots the full v4 manifest. Old links without a manifest remain valid but contribute no node sets.

`node_sets` contains objects with `id`, optional `parameters`, and `nodes`. Each node declares a symbolic `ref`, `core`, `layer`, optional `group`, and complete `profile`. The default group is the package name after `/`; MuJoCo declares `mujoco` explicitly. A parameter has `{ "type": "string", "default": "optional" }`; `${name}` in any profile string is replaced by the provided value. A profile reference to `@ref` resolves to the corresponding full graph path, including in `references`, `inputs`, and adapter settings. IsOS injects `profile.package={id,version}` only for node types contributed by that package and rejects conflicting pins. Built-in EOS and IsosUI nodes in a package set remain unpinned. Profile fields follow the [EOS node graph contract](node-graph.md).

Package installation only makes node sets available. `isos pkg node-sets` lists them, and `isos pkg preview-set ID VERSION SET_ID graph.json --values values.json` prints a composed graph and `added_paths`. The HTTP preview has the same behavior. Applying still uses the revision-checked graph API. Removal rejects any saved graph node with the exact package pin.

Package `node_contributions.adapter_families` is an optional array of adapter implementation descriptors. Each descriptor requires `id` (nonempty stable string, unique within a release), `device_kinds` (nonempty array of device-class strings such as `camera`, `base`, `manipulator`), `transport` (nonempty transport string such as `unix_socket`), and `description` (nonempty human-readable purpose). It may include `supported_primitives` (array of exact `name@version` strings) and `settings_schema` (JSON Schema object for adapter settings). IsOS preserves package provenance in the verified sidecar. EOS computes `compiled` and an unavailable `reason` in its catalog; a package cannot claim that its adapter is compiled. Generic hardware node types remain independent of adapter identity.

## 0.x target: verified options and package-specific nodes — not implemented in full

The v4 release manifest remains the distribution envelope. Its `schema_version`, `id`, `kind`, `version`, `source_commit`, `source_url`, `release_url`, `contracts`, `assets`, and optional `activation` retain the meanings above; the `1.0.1` sample is historical, not the current release target. An approved registry entry provides metadata and an asset URL; IsOS downloads the exact asset, verifies host selectors, length and SHA-256, extracts only safe regular paths, and writes a receipt. It then snapshots contributed type/data/adapter declarations with the exact release pin. A node set is a template only; it is composed into a draft and applied through revision control. A package cannot create an actuator route or run arbitrary code merely by contributing a node type.

The target package descriptor adds an optional resource index for selector settings; MuJoCo initially supplies scene and device IDs already present in `backend-v2.json`. A future resource index must use the same fields below and is validated against files inside the verified bundle. The descriptor filename stays a package compatibility identity until the package itself migrates; documentation filename cleanup does not rename it.

| Resource field | JSON type / unit | Purpose and owner | Default and validation | Compatibility |
| --- | --- | --- | --- | --- |
| `kind` | string / resource family | Package declares `scene`, `device`, or later registered family | Required known family | New families require a resolver |
| `id` | string / stable selector value | Package identity for graph setting | Required unique within kind and release; nonblank, no path separators | Existing graph IDs stay valid |
| `label` | string / — | Operator display | Nonblank; defaults to ID for old scene/device descriptors | Display may change |
| `description` | string / — | Function and usage | Nonblank on new resources; old descriptor gets catalog fallback | No graph rewrite |
| `path` | relative package path or null / — | Asset bytes for file resources | Required safe regular path for scene/file; null for logical devices | Never stored in graph or sent to UI |
| `sha256` | 64 hex chars or null / digest | Detect asset change | Required for new file resources; old MuJoCo descriptor uses verified bundle fingerprint | Rechecked at Apply and Run |

For a registered `Setting.selector={provider:"package",resource:"scene",package_from:"node.package"}`, IsOS returns the package's stable scene IDs through [node-options](local-api.md#release-target-setting-options--not-implemented). If the release is absent, unverified, changed, or incompatible with its compiled EOS adapter, the option remains visible with an unavailable reason and Apply fails. Package metadata stays separate from typed hardware payloads; EOS's internal adapter registry determines whether `mujoco-v2` can actually connect. The new `@v0.1` process type is contributed by the MuJoCo package, while generic camera/base/manipulator types remain EOS-owned.
