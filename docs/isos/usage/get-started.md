# Start an IsOS deployment

[中文](../../../docs_cn/isos/usage/get-started_cn.md)

On Linux with a user systemd session and Rust 1.98 or newer, build and install the IsOS CLI and local API server. The editor source is maintained in the public IsosUI repository; the standard IsOS installation does not include UI files.

    ./build.sh
    ./install.sh
    isos init
    isos open

`isos open` uses the hosted editor by default. For an offline editor, install Node.js and npm, set `ISOSUI_REPO_PATH` to a clean IsosUI checkout, then build and pin that commit. `isos version check` reports the IsosUI pin separately from EOS and EGIA. A missing pin uses the hosted editor; a damaged pin reports an error.

    isos build isosui
    isos version pin isosui <commit>

For local source builds, set `EOS_REPO_PATH` and `EGIA_REPO_PATH` to the committed checkout paths. The checkout wrapper reads `.env` if present. IsOS builds explicit candidates and never installs EOS system dependencies unless `--install-deps` is passed.

    isos build eos
    isos build egia
    isos version pin eos <exact-commit>
    isos version pin egia <exact-commit>
    isos version check

Use the editor to enable EOS, its logical UDS endpoint, and EGIA. Validate and Apply are separate; a stale editor revision gets a conflict. The configuration server remains usable while both runtimes are stopped.

    isos run
    isos status
    isos logs egia
    isos stop

`run` starts enabled graph nodes in dependency order and checks the pinned build digests and EOS/EGIA contracts. EGIA connects directly to EOS's private UDS. Stopping reverses graph order. To change an active deployment, apply the graph and restart explicitly. The first-party demo EOS binding can start without a simulator, but demo primitives remain unavailable until a simulator service is running.
