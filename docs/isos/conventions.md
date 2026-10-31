# IsOS development conventions

IsOS follows [Island coding conventions v1.2.0](https://github.com/Island-Team/Island-eos/blob/main/docs/island/conventions.v1.2.0.md), adapted to this Rust manager. This page is the local rule for the repository; update its [Chinese companion](../../docs_cn/isos/conventions_cn.md) in the same change.

Introduce IsOS, EOS, EGIA, packages, and specialized terms before a reader must use them. Keep operator guides under usage, wire contracts under interfaces, and release or migration work under maintenance. Interface pages identify owner, caller, transport, input, output, lifecycle, and observable errors with small valid request and response examples. Keep English and Chinese reading paths aligned.

Use 2 spaces for Markdown, JSON, and configuration indentation. Rust follows cargo fmt and the official Rust style guide. Keep inline comments short and explain only invariants that are not obvious from the code. Python tooling defaults to Python 3.12 unless its own project pins another supported version; IsOS launch commands may select Conda, uv, or an explicit interpreter and must not assume system Python.

Before committing, run cargo fmt --check, cargo clippy --all-targets -- -D warnings, and cargo test. Add tests for graph ordering, package validation, process ownership, and API boundary changes. Keep runtime state under XDG paths, not in the repository. Use small Conventional Commits; a public PR can be squash-merged after review.

The public IsosUI repository owns the editor source and static build. IsOS owns the CLI, loopback API, and configured path to one verified UI bundle. Do not copy editor source here or package UI assets into the standard IsOS release. Keep the UI bundle path independent of EOS/EGIA graph selection.
