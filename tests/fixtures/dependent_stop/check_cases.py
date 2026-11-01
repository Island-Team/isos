#!/usr/bin/env python3
"""Validate the design-only R06 IsOS stop inventory."""

import json
from pathlib import Path


root = Path(__file__).resolve().parent
data = json.loads((root / "cases.json").read_text())
assert data["schema_version"] == 1
cases = data["cases"]
assert len(cases) >= 31
ids = [case["id"] for case in cases]
assert len(ids) == len(set(ids))
reasons = {
    "child_exit_unexpected", "stop_signal_failed", "stop_deadline_exceeded",
    "stop_forced_kill", "stop_group_unreaped", "eos_stop_unconfirmed",
    "eos_receipt_missing", "control_grant_retire_failed", "stop_result_persist_failed", "stop_interrupted",
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
    if case.get("must_not_signal_reused_pgid") or case.get("must_not_signal_saved_pgid"):
        assert case["expected_outcome"] == "unknown", case
    if case.get("signal_pinned_original_group"):
        assert case["id"] == "leader_exited_unreaped_before_stop", case
        assert case["expected_outcome"] == "unknown", case
    if case.get("new_run_record_before_spawn"):
        assert case["saved_phase"] == "running" and case["expected_outcome"] == "no_stop", case
    if "cleanup_at_ms" in case:
        assert case["cleanup_at_ms"] == case["stop_timeout_ms"] - 2000, case
        assert case["max_kill_reap_ms"] + case["min_persist_ms"] == 2000, case
assert {"clean_reverse_dependency", "eos_dual_ack_unknown", "provider_crash", "restart_old_stop_unknown", "log_collector_outage", "simulator_crash", "socket_cleanup_failure"}.issubset(ids)
assert {"leader_exited_unreaped_before_stop", "leader_reaped_before_group_signal", "cleanup_window_starts_before_deadline", "crash_after_durable_stop_intent", "crash_during_stop", "new_run_stop_state_reset", "crash_between_p07_publish_and_state_reset", "stop_intent_fsync_failure", "manager_crash_with_running_record"}.issubset(ids)
print(f"R06 IsOS stop fixture: {len(cases)} cases passed")
