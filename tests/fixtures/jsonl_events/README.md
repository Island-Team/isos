# P06 shared JSONL event fixtures

[中文](README_cn.md) · [Owner contract](../../../docs/isos/interfaces/events.md) · [Design](../../../docs/isos/design/events.md)

`cases.json` freezes 30 success, rejection, suppression and lifecycle scenarios for IsOS P06. The case names are stable inputs for implementation tests. The fixture checker validates the case inventory and representative raw-child and stored-record wire boundaries, including the 8,064/8,065 and 8,192/8,193 byte edges. Run `python3 tests/fixtures/jsonl_events/check_cases.py`. This does not claim that the runtime collector is implemented.

Implement each named case against `LogStore`, `Supervisor`, manager status and a selected stub child. In particular, rejected raw or normalized-overfull records increment a bounded `log_ingest_rejected` reason counter without copying the line; draining an overlong raw line must leave the next valid line readable. Bind selected `eos`, `bridge`, `config` and `egia` sources to direct-child roles, compare `run_id` with the active run, and reject raw `pid` and node member pointers. Test manager-session prelaunch failure, exact package pin and verified receipt provenance, candidate/recovery run IDs, suppression, rotation and nonblocking stop. Replay real EOS and EGIA stderr as the final owner boundary test.
