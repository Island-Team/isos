#!/usr/bin/env python3
"""Check P06 design fixtures and representative JSONL boundary examples.

This is contract evidence, not a test of the future IsOS collector.
"""

import json
import re
from pathlib import Path


ROOT = Path(__file__).parent
CASES = json.loads((ROOT / "cases.json").read_text())
EXPECTED_IDS = {
    "manager_preflight_failure", "package_verified_pin_receipt",
    "package_unverified_receipt", "start_new_run", "start_idempotent",
    "restart_preflight_failure", "restart_recovery", "eos_runtime_ready",
    "bridge_ready", "config_ready", "egia_ready", "child_source_spoof",
    "child_run_mismatch", "child_pid_spoof", "node_pointer_member",
    "node_pointer_invalid_escape", "raw_line_at_8064", "raw_line_at_8065",
    "normalized_line_at_8192", "normalized_line_at_8193",
    "next_line_after_drain", "unknown_top_level", "duplicate_json_key",
    "secret_env_value", "model_prompt", "raw_frame", "periodic_duplicate",
    "first_failure", "stop_during_log_outage", "source_rotation",
}
assert CASES["schema"] == "isos-jsonl-events-p06-fixtures-v1"
rows = CASES["cases"]
assert {row["id"] for row in rows} == EXPECTED_IDS
assert len(rows) == len(EXPECTED_IDS)
for row in rows:
    assert set(row) == {"id", "kind", "expected", "assertion"}
    assert row["expected"] in {"accept", "reject", "suppress"}
    assert row["assertion"]

raw = {
    "timestamp": "2026-11-01T10:00:01.000Z",
    "level": "info",
    "source": "eos",
    "core": "eos",
    "node_path": "/cores/eos/layers/process/groups/mujoco/nodes/sim",
    "event_code": "eos_ready",
    "run_id": "r7-cde",
    "correlation_id": None,
    "message": "EOS runtime ready",
    "fields": {"state": "ready"},
}


def line(event):
    return (json.dumps(event, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def valid_child(event, role="eos", run="r7-cde"):
    required = {"timestamp", "level", "source", "core", "event_code", "run_id", "message"}
    allowed = set(raw)
    if not required <= set(event) or not set(event) <= allowed:
        return False
    if event["source"] != role or event["run_id"] != run:
        return False
    if event["core"] != ("egia" if role == "egia" else "eos"):
        return False
    if not re.fullmatch(r"[a-z0-9_]{1,64}", event["event_code"]):
        return False
    pointer = event.get("node_path")
    if pointer is not None:
        parts = pointer.split("/")
        if len(parts) != 9 or parts[1::2] != ["cores", "layers", "groups", "nodes"]:
            return False
        # The admitted node-profile segment grammar excludes slash and tilde.
        if any(not p or "~" in p for p in parts[2::2]):
            return False
        if len(pointer.encode()) > 512:
            return False
    return len(line(event)) <= 8064


assert valid_child(raw)
assert valid_child({key: value for key, value in raw.items() if key not in {"node_path", "correlation_id", "fields"}})
assert "pid" not in raw
assert not valid_child({**raw, "source": "bridge"})
assert not valid_child({**raw, "run_id": "r-other"})
assert not valid_child({**raw, "pid": 1})
assert not valid_child({**raw, "node_path": raw["node_path"] + "/package"})
assert not valid_child({**raw, "node_path": "/cores/eos/layers/process/groups/mujoco/nodes/~2"})

# A child emitter trims sanitized UTF-8 text to the exact wire limit.
base = len(line({**raw, "message": ""}))
at_limit = {**raw, "message": "x" * (8064 - base)}
assert len(line(at_limit)) == 8064 and valid_child(at_limit)
over_limit = {**at_limit, "message": at_limit["message"] + "x"}
assert len(line(over_limit)) == 8065 and not valid_child(over_limit)
assert valid_child(raw)  # next line remains parseable after draining over_limit

stored = {**raw, "pid": 4242}
stored_base = len(line({**stored, "message": ""}))
stored_limit = {**stored, "message": "x" * (8192 - stored_base)}
assert len(line(stored_limit)) == 8192
assert len(line({**stored_limit, "message": stored_limit["message"] + "x"})) == 8193

# The stored bound is checked after normalization even for an otherwise valid raw line.
assert len(line({**at_limit, "pid": 4242})) <= 8192
print(f"P06 fixture contract: {len(rows)} cases; raw 8064/8065 and stored 8192/8193 checked")
