#!/usr/bin/env python3
"""Validate the design-only R06 IsOS stop inventory."""

import json
from pathlib import Path


root = Path(__file__).resolve().parent
data = json.loads((root / "cases.json").read_text())
assert data["schema_version"] == 1
cases = data["cases"]
assert len(cases) >= 20
ids = [case["id"] for case in cases]
assert len(ids) == len(set(ids))
reasons = {
    "child_exit_unexpected", "stop_signal_failed", "stop_deadline_exceeded",
    "stop_forced_kill", "stop_group_unreaped", "eos_stop_unconfirmed",
    "eos_receipt_missing", "control_grant_retire_failed", "stop_result_persist_failed",
}
for case in cases:
    assert case["expected_outcome"] in {"confirmed", "unknown", "no_stop"}, case
    assert case["expected_stop_order"] == list(dict.fromkeys(case["expected_stop_order"])), case
    assert set(case["expected_stop_order"]).issubset(set(case["selected"])), case
    if case["expected_outcome"] == "unknown":
        assert case["expected_reason"] in reasons, case
        assert case["expected_event"] in {"deployment_stop_unconfirmed", "unavailable"}, case
    else:
        assert "expected_reason" not in case, case
    if case.get("automatic_restart_blocked") or case.get("candidate_spawned") is False:
        assert case["expected_outcome"] == "unknown", case
    if case.get("old_run_unchanged"):
        assert case["expected_stop_order"] == [], case
assert {"clean_reverse_dependency", "eos_dual_ack_unknown", "provider_crash", "restart_old_stop_unknown", "log_collector_outage", "simulator_crash", "socket_cleanup_failure"}.issubset(ids)
print(f"R06 IsOS stop fixture: {len(cases)} cases passed")
