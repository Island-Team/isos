# Integration roadmap

[中文](../../../docs_cn/isos/maintenance/roadmap_cn.md)

The [integration plan](plan.md) records implemented behavior. Remaining milestones are:

1. Qualify and publish exact IsOS, IsosUI, EOS, and MuJoCo artifacts on supported Linux targets, including a product-mode clean-home test without npm or source checkout.
2. Run real MuJoCo and configured Island-models service checks, including failure, stop, and descendant cleanup.
3. Version and test cross-repository API, graph, node-type, and service contracts before release.
4. Integrate EGIA runtime control after its contract is available. EGIA common nodes are design-only today.
5. Design future service graph representation without coupling the current independent TOML service manager to EOS or EGIA nodes.
6. Keep the optional user systemd unit as a launcher for the same product `isos serve` command.
