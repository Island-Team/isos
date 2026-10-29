# Coding-agent orientation

Read README.md and docs/isos/README.md before changing IsOS. Follow
docs/isos/conventions.md and the owning interface contract for the code you
touch. Preserve the versioned JSON and HTTP/Unix API fields, and update the
English and Chinese docs together when behavior changes.

Keep process state under XDG runtime/data locations, never in the source
checkout. Run cargo fmt --check, cargo clippy --all-targets -- -D warnings,
and cargo test before handoff. Use Conventional Commits.

IsOS owns the CLI, loopback API, and local UI build/pin integration. IsosUI owns
the public editor source and static build. Do not copy editor source into this
repository or add UI assets to the standard IsOS release. A local IsosUI build
is optional and pinned independently of the EOS/EGIA deployment graph.
