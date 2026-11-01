# `isos / process / component / managed-component`

[中文](../../../../../../../docs_cn/isos/layers/process/groups/component/nodes/managed-component_cn.md) · [Settings and graph fields](../../../../../interfaces/node-graph.md#component-process-node)

**Type:** current `isos.process.component@v1.1`; target new registrations use `isos.process.component@v0.1`. **Description:** Starts one configured EOS or EGIA component binary from a verified core installation; observes readiness and stops its process group. The instance ID is the final path segment and its own description states the deployment role. This node never accepts an arbitrary executable.

**Inputs and outputs:** Dependency references (`depends_on`) determine startup order; no command or physical data port is exposed. Status projects PID, readiness, restart count, last error, and active graph revision through the [local API](../../../../../interfaces/local-api.md). `role` and bounded `args` select the installed binary; the graph contract owns all types, limits, and defaults.

**Lifecycle:** Validate the role and package/binary before Apply; at Run start prerequisites first, then the direct child and its readiness probe. A failed prerequisite blocks the child. If readiness times out, report the node path and error, stop dependents, then apply bounded restart policy. Stop dependents before providers, send TERM to the process group, wait two seconds, then KILL remaining descendants. A PID or open TCP port alone does not prove EGIA's required EOS discovery completed.

**Compatibility:** Existing safe `@v1.1` graph instances retain their bytes and meaning until explicit revision-checked migration. The obsolete `@v1.0` arbitrary `command` setting is rejected with guidance; it is never executed by a new reader. New optional settings need a default; changing `role`, readiness, or stop semantics requires a new type ID.
