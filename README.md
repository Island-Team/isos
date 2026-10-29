# IsOS

IsOS is the local manager for Island EOS and EGIA. It keeps the package library, deployment configuration, and process lifecycle in one place. EOS and EGIA remain independent runtimes: you can start EOS alone, attach its CLI, or start a configured EOS and EGIA stack.

This repository is an early, working manager core. Its command line, graph validator, release validator, package store, systemd lifecycle, and owner-only Unix socket API are implemented. IsOS manages EOS and EGIA components alongside external service and backend packages. Automatic migration from old TOML files is future work.

## Install

Linux with a user systemd session and Rust 1.98 or newer. For local core checkouts, copy `.example.env` to `.env` and set the EOS and EGIA repository paths. Set `ISOSUI_REPO_PATH` only when building the optional local editor. The checkout wrappers load `.env`; Git ignores it.

    ./build.sh
    ./install.sh
    isos doctor

The installer places `isos` in Cargo's bin directory. If that directory is not on `PATH` but `~/.local/bin` is, it creates a link there. Otherwise, add the install bin directory (normally `~/.cargo/bin`) to `PATH` and open a new shell.

The IsOS installation contains the CLI and local API server. `isos open` uses the hosted IsosUI editor at `https://www.os-island.com/en/isos` unless a local build has been pinned. To use the editor offline, set `ISOSUI_REPO_PATH` to a committed [IsosUI](https://github.com/Island-Team/IsosUI) checkout, install Node.js and npm, then run `isos build isosui` and `isos version pin isosui <commit>`. IsOS stores the verified bundle under XDG data and serves it from `127.0.0.1:16000`. The UI pin is independent of the EOS/EGIA deployment graph.

Before installing the CLI, run commands from this checkout with `./bin/isos <command>` or `./isos.sh <command>`.

The first platform is Linux. The default paths follow XDG:

| Purpose | Default |
| --- | --- |
| Deployment graph | ~/.config/isos/deployments/default.json |
| Generated files | ~/.local/share/isos/generated/default/ |
| Installed packages | ~/.local/share/isos/packages/ |
| Active state and API socket | $XDG_RUNTIME_DIR/isos/ |

For isolated testing, set ISOS_HOME to an absolute directory; it then contains config/, data/, and run/. Set ISOS_DEPLOYMENT or pass --deployment <name> to select a different graph.

## First deployment

The IsOS editor stays available while EOS and EGIA are stopped. Point the checkout wrapper at the sibling repositories with `.env`, then:

    isos init
    isos open
    isos build eos
    isos build egia
    isos version pin eos <exact-commit-or-version>
    isos version pin egia <exact-commit-or-version>
    isos version check

`init` creates a disabled v2 starter graph and starts the local IsOS service on `127.0.0.1:16000`. The editor connects to that port without a token and has explicit Validate and Apply actions. Enable `eos`, `eos-uds`, and `egia` after pinning both builds. The `eos.uds` node is a logical endpoint on the EOS process. IsOS renders EOS and EGIA configuration without starting either runtime. A local EOS build supplies a first-party demo binding; its primitives need a simulator service to become ready.

    isos config validate ~/.config/isos/deployments/default.json
    isos run
    isos status
    isos stop

`run` checks pinned artifact digests and EOS/EGIA contract versions, then starts EOS before the UDS endpoint and EGIA. `stop` reverses that order. A running deployment keeps its active graph snapshot until an explicit restart. `isos config migrate` backs up a v1 graph before converting it to v2; it rejects unmappable fields. `start` remains an alias for `run`.

## Commands

    isos init|open|run|stop|restart|status [--json]
    isos build eos|egia|all [--install-deps]
    isos build isosui
    isos version check|pin <eos|egia|isosui> <version>
    isos eos start|enter|stop|restart|status [--json]|shutdown
    isos egia start|stop|restart|status [--json]
    isos config init|validate|apply|migrate|show
    isos pkg search|info|install|link|list|verify|upload|publish|remove
    isos skills list|info|status|install|remove
    isos services list|info|status|install|remove
    isos backends list|info|status|install|remove
    isos logs <node>
    isos doctor
    isos serve

## Tab completion

`./install.sh` installs Bash, Zsh, and Fish completion scripts. It adds a loader to an existing Bash or Zsh startup file (or to your login shell's startup file); Fish finds its script in the standard completions directory. Open a new shell after installation to complete IsOS commands, subcommands, and options with Tab.

isos services is the model service library; isos backends is the hardware and simulator library. The three library commands inspect graph nodes and manage releases of their matching package kind. Install and remove use the same verified package store as isos packages. EOS and EGIA are components. EOS's Inner Skills Community (ISC) is a separate runtime mechanism for skill communication, and ss is its execution monitor.

Start with the [documentation catalog](docs/README.md). The Chinese quick-start is in [README_cn.md](README_cn.md).

## Development

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

The manager never kills processes by matching a name or port. It starts scoped user-systemd units named isos-<deployment>-<node>.service and stores the active graph snapshot separately from the selected graph. Stopping follows that active snapshot, even when configuration has changed since startup.
