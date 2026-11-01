#!/usr/bin/env python3
"""Design-fixture self-check for the P07 owner wire contract; no runtime oracle."""
import json
from pathlib import Path

p = Path(__file__).with_name("cases.json")
d = json.loads(p.read_text())
assert d["contract"] == "isos-log-cursors-p07-v1"
assert d["endpoint"] == "/api/isos/v1/logs" and d["view"] == "current"
assert d["response_keys"] == ["scope", "anchor", "lines", "next_cursor", "rotated", "diagnostics"]
assert d["scope_keys"] == ["kind", "run_id", "active_revision"]
assert d["anchor_keys"] == ["kind", "state", "event"]
assert d["filter_order"] == ["scope", "anchor", "run_id", "source", "level", "node_path", "correlation_id"]
assert d["diagnostic_order"] == ["log_run_changed", "log_cursor_expired", "log_collector_gap", "log_anchor_unavailable"]
assert d["bounds"] == {"default_limit": 100, "max_limit": 200, "max_scan_bytes": 65536, "max_cursor_ascii_bytes": 512}
expected_names = {
    "manager_package_failure_filter_bypass", "manager_apply_failure", "manager_validate_only_not_anchor",
    "run_pending_empty", "run_ready_first", "run_failure_before_ready", "run_filtered_anchor_and_progress",
    "rotation_retained", "rotation_expired", "new_admitted_run_rebase", "stop_retains_last_run",
    "failed_retains_last_run", "collector_outage_recovery", "anchor_write_failure",
    "legacy_socket_cannot_anchor", "cursor_filter_mismatch", "cursor_tampered", "legacy_cursor_rejected",
}
cases = d["cases"]
assert len(cases) == len(expected_names)
assert {c["name"] for c in cases} == expected_names
for c in cases:
    assert set(c) == {"name", "phase", "query", "expected"} or set(c) == {"name", "phase", "query", "expected_error"}
    if "expected_error" in c:
        assert c["expected_error"]["http"] == 400
        assert c["expected_error"]["path"] == "/query/cursor"
        assert c["expected_error"]["code"] in {"log_cursor_invalid", "log_cursor_filter_mismatch"}
        continue
    e = c["expected"]
    assert e["scope_kind"] in {"manager_session", "deployment_run"}
    assert e["anchor_state"] in {"pending", "found", "unavailable"}
    if e["anchor_state"] == "pending":
        assert e.get("lines") == [] and e.get("anchor_code") is None
    if e["anchor_state"] == "unavailable":
        assert any(x["code"] == "log_anchor_unavailable" for x in e["diagnostics"])
    codes = [x["code"] for x in e["diagnostics"]]
    assert codes == sorted(codes, key=d["diagnostic_order"].index)
    assert all(x["count"] is None or (isinstance(x["count"], int) and x["count"] >= 0) for x in e["diagnostics"])
assert next(c for c in cases if c["name"] == "manager_package_failure_filter_bypass")["expected"]["anchor_source"] == "packages"
assert next(c for c in cases if c["name"] == "stop_retains_last_run")["expected"]["process_status_run_id"] == "r7"
assert next(c for c in cases if c["name"] == "failed_retains_last_run")["expected"]["process_status_run_id"] == "r7"
print(f"P07 contract fixtures OK: {len(cases)} cases")
