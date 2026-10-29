# Start an IsOS deployment

IsOS is a local manager for EOS and EGIA. It uses a JSON deployment graph to select exact packages and start only the enabled nodes. On Linux, install it with Rust 1.98 or newer and a user systemd session:

    cargo install --path .
    isos doctor
    isos config init

The starter graph is at ~/.config/isos/deployments/default.json. It keeps EOS, its north HTTP sidecar, and EGIA disabled until you provide launch commands and the EOS north HTTP URL. Follow the [graph contract](../interfaces/deployment-graph.md) for the exact fields. Python 3.12 is the default for Python components; the launch command may select a different supported interpreter through a Conda environment, an uv environment, or an explicit executable. IsOS does not hardcode system Python.

For a Python service, launch.command can begin with /home/user/.conda/envs/model-py312/bin/python, or with uv run --python 3.12 --project /path/to/service python. Put the rest of the module command in the same array. Resolve the environment before starting the deployment so systemd does not prompt or download dependencies during startup.

Validate and apply the edited graph, then start the stack:

    isos config validate ~/.config/isos/deployments/default.json
    isos config apply ~/.config/isos/deployments/default.json
    isos start
    isos status
    isos eos enter

Apply writes formatted JSON and generated TOML. It does not restart running processes. Run isos restart when you deliberately want the new graph to replace an active deployment. Run isos eos start to start EOS and its dependencies without starting EGIA; isos eos enter attaches to an already-running EOS. The EOS CLI's shutdown command stops the deployment-owned EOS closure, while leave, quit, and exit close only the client.

To manage packages, continue with [package operations](packages.md). To move an existing EOS/EGIA stack, use the [migration guide](../maintenance/migration.md).

[中文](../../../docs_cn/isos/usage/get-started_cn.md)
