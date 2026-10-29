# Release the IsOS CLI

[中文](../../../docs_cn/isos/maintenance/cli-release_cn.md)

The manual `IsOS CLI release` workflow packages the Linux x86_64 CLI and loopback API server. It does not bundle IsosUI. The public IsosUI repository releases its own static bundles; Island-web promotes a hosted bundle separately.

1. Merge and verify the intended IsOS commit on `master`. Confirm `Cargo.toml` has the intended version and that the `v<version>` tag does not already exist.
2. Run the workflow on `master` with `phase=prepare`. It runs format, Clippy, tests, and a locked release build, then uploads a temporary archive, `SHA256SUMS`, and source commit metadata. Review the run and download/inspect its artifact.
3. Run the same workflow on `master` with `phase=publish` and the successful prepare run ID. It checks the artifact version, source commit ancestry, and SHA-256, then creates the GitHub Release and tag. Do not run publish until the prepared archive is approved.

The release archive contains `isos`, `LICENSE`, and `README.md`. Install the binary into a directory on `PATH`; `./install.sh` is for source checkouts and is not in the archive. Use `isos build isosui` and `isos version pin isosui <commit>` for an optional verified offline editor.
