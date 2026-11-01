# G06 frozen edge fixtures

`valid_0_1_0.json` and `catalog.json` fix the exact EOS camera `frame` →
camera gate `frame` route and an unrouted EGIA-local descriptive edge.
`vision_bridge_unregistered.json` adds camera gate `preview` → IsosUI Vision:
the Vision type is absent from the M1 catalog; even a synthetic matching
Vision descriptor cannot authorize the undeclared cross-core bridge.
`fixed_command_qos.json` adds a Dispatcher primitive binding → base command
edge with explicit priority and must fail at the array source's `/0/priority`.

The test mutates these fixtures for type, unit-schema, placement, transport,
cardinality, cycle, route and QoS negatives. These bytes are checked into Git;
the clean-home replay additionally checks HTTP, CLI, migration, and Run gates
against the current owner executables.
