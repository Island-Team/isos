# IsOS

IsOS is the local manager for Island EOS and EGIA. It keeps the package library, deployment configuration, and process lifecycle in one place. EOS and EGIA remain independent runtimes: you can start EOS alone, attach its CLI, or start a configured EOS and EGIA stack.

This repository is an early, working manager core. Its command line, graph validator, release validator, package store, systemd lifecycle, owner-only Unix socket API, and paired loopback WebUI bridge are implemented. The browser pages and automatic migration from old TOML files are still being built in the companion repositories.

## Install

Linux with a user systemd session and Rust 1.98 or newer:

    cargo install --path .
    isos doctor

The first platform is Linux. The default paths follow XDG:

| Purpose | Default |
| --- | --- |
| Deployment graph | ~/.config/isos/deployments/default.json |
| Generated files | ~/.local/share/isos/generated/default/ |
| Installed packages | ~/.local/share/isos/packages/ |
| Active state and API socket | $XDG_RUNTIME_DIR/isos/ |

For isolated testing, set ISOS_HOME to an absolute directory; it then contains config/, data/, and run/. Set ISOS_DEPLOYMENT or pass --deployment <name> to select a different graph.

## First deployment

    isos config init
    isos config show

The starter graph has disabled EOS, north HTTP sidecar, and EGIA nodes. Edit their launch.command arrays to point at installed releases or development checkouts, set the EOS settings.eos_config and settings.north_http_url, then set enabled to true. For EGIA, add its launch arguments and ensure its contract matches EOS. The dependency edges are already present.

    isos config validate ~/.config/isos/deployments/default.json
    isos config apply ~/.config/isos/deployments/default.json
    isos start
    isos status
    isos eos enter

isos config apply formats and saves the graph and generates a compatible stack.toml. It does not restart running services. isos restart is an explicit operation. isos eos enter attaches only when EOS is running.

Inside the EOS CLI, leave, quit, and exit leave the client. shutdown stops EOS, dependent EGIA, and deployment-owned dependencies while preserving separately started services. IsOS also closes other CLI sessions entered through isos eos enter.

## Commands

    isos start|stop|restart|status [--json]
    isos eos start|enter|stop|restart|status [--json]|shutdown
    isos egia start|stop|restart|status [--json]
    isos config init|validate|apply|show
    isos packages search|info|install|link|list|verify|remove
    isos skills list|info|status|install|remove
    isos services list|info|status|install|remove
    isos backends list|info|status|install|remove
    isos logs <node>
    isos doctor
    isos daemon
    isos ui

isos services is the model service library; isos backends is the hardware and simulator library. The three library commands inspect graph nodes and manage releases of their matching package kind. Install and remove use the same verified package store as isos packages. EOS and EGIA are components. EOS's Inner Skills Community (ISC) is a separate runtime mechanism for skill communication, and ss is its execution monitor.

Start with the [documentation catalog](docs/README.md). The Chinese quick-start is in [README_cn.md](README_cn.md).

## Development

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

The manager never kills processes by matching a name or port. It starts scoped user-systemd units named isos-<deployment>-<node>.service and stores the active graph snapshot separately from the selected graph. Stopping follows that active snapshot, even when configuration has changed since startup.
