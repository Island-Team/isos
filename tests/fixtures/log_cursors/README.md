# P07 current-run log cursor fixtures

[中文](README_cn.md) · [Owner interface](../../../docs/isos/interfaces/log-cursors.md) · [Design](../../../docs/isos/design/log-cursors.md)

`cases.json` freezes the opt-in endpoint, response keys, scope and anchor fields, filter order, diagnostic order, bounds and 21 positive/negative wire cases and nine executable state scenarios. `check_cases.py` executes a reference admission/scope selector over the state scenarios and checks wire fixture consistency with `python3 tests/fixtures/log_cursors/check_cases.py`; it is contract evidence, not a runtime test. Implementation tests must drive real LogStore/HTTP/CLI paths and compare observable pages and status with these cases. The `lc1.*` values in queries are scenario labels, not serialized tokens.
