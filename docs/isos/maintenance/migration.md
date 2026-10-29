# Migration from legacy launchers

[中文](../../../docs_cn/isos/maintenance/migration_cn.md)

The old EOS island-launch and island-pkg tools, and EGIA bin/island combined launcher, are retained as compatibility wrappers in their source repositories. IsOS owns new stack composition. These wrappers may still support legacy-only flags while an existing deployment is being migrated.

1. Install IsOS with cargo install --path . and run isos doctor.
2. Run isos config init. Keep a copy of existing EOS stack.toml, eos.toml, EGIA island.toml, and model service configuration.
3. Replace starter graph launch commands with the corresponding installed EOS and EGIA commands. Put model servers in service nodes and hardware or simulators in backend nodes. Add consumer-to-provider edges.
4. Select exact package versions using isos packages install or link, then record each package ID and version in its graph node.
5. Run isos config validate <file> and isos config apply <file>.
6. Stop the old launchers, then run isos start and isos status. Use isos eos enter for an attached diagnostic CLI.

IsOS does not yet import the old TOML automatically. It also does not move model weight files; service launch commands continue to reference those existing files. Do not run the old combined EGIA launcher and IsOS against the same deployment at once. The generated stack.toml is an adapter for existing EOS tooling; edit the JSON graph, not generated TOML.
