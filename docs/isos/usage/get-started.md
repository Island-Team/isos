# Start an IsOS deployment

[中文](../../../docs_cn/isos/usage/get-started_cn.md)

On Linux with a user systemd session and Rust 1.98 or newer, build and install IsOS with the standalone Island-web bundle. `ISOS_WEB_BUNDLE` selects a release artifact; local builds use the sibling Island-web checkout.

    ./build.sh
    ./install.sh
    isos init
    isos open

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
