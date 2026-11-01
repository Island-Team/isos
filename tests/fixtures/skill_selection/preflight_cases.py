#!/usr/bin/env python3
"""Check the IsOS owner-side R04 preflight design table and closed wire examples."""
import json
from pathlib import Path

root = Path(__file__).resolve().parent
raw = (root / "preflight_cases.json").read_text()

def no_duplicates(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


data = json.loads(raw, object_pairs_hook=no_duplicates)
assert set(data) == {"schema_version", "eos_design_commit", "success_result", "rejection_result", "cases"}
assert data["schema_version"] == 1
assert len(data["eos_design_commit"]) == 40
success = data["success_result"]
assert set(success) == {"schema_version", "ok", "active_revision", "generation", "run_id", "selection_sha256", "libraries"}
assert success["schema_version"] == 1 and success["ok"] is True
for name in ("active_revision", "generation", "selection_sha256"):
    assert len(success[name]) == 64 and all(c in "0123456789abcdef" for c in success[name])
assert len(success["libraries"]) == 1
assert set(success["libraries"][0]) == {"package_id", "package_version", "library", "library_sha256", "abi_version", "skill_count"}
assert success["libraries"][0]["abi_version"] == 2
assert 1 <= success["libraries"][0]["skill_count"] <= 256
rejection = data["rejection_result"]
assert rejection == {"schema_version": 1, "ok": False, "stage": "abi", "code": "abi_descriptor_invalid"}
cases = data["cases"]
assert [case["id"] for case in cases] == [f"R04-IPF{i:02}" for i in range(1, 19)]
allowed = {None, "skill_catalog_preflight_rejected", "skill_catalog_preflight_invalid_output", "skill_catalog_preflight_timeout", "skill_catalog_preflight_cleanup_unconfirmed"}
assert all({"id", "mutation", "expected", "isos_code"} <= set(case) and case["isos_code"] in allowed for case in cases)
closed = {
    "input": {"preflight_arguments_invalid", "preflight_grants_invalid", "preflight_profile_invalid"},
    "selection": {"selection_invalid", "selection_revision_mismatch", "selection_manifest_invalid", "selection_digest_mismatch", "selection_handoff_incomplete"},
    "package": {"package_invalid", "package_changed", "package_unsafe", "library_changed"},
    "elf": {"library_not_elf", "library_dependency_unsafe"},
    "worker": {"worker_spawn_failed", "worker_sandbox_failed", "worker_inspection_failed", "worker_timeout", "worker_reap_failed"},
    "abi": {"abi_library_invalid", "abi_descriptor_invalid", "abi_schema_invalid", "abi_capability_unowned"},
    "catalog": {"catalog_duplicate_identity", "catalog_capacity", "catalog_broker_unavailable", "catalog_binding_unavailable", "catalog_list_oversize"},
}
for case in cases:
    if "eos_stage" in case or "eos_code" in case:
        assert case["eos_code"] in closed[case["eos_stage"]]
        assert case["isos_code"] == "skill_catalog_preflight_rejected"
assert len(json.dumps(success, separators=(",", ":")).encode()) < 16384
assert len(json.dumps(rejection, separators=(",", ":")).encode()) < 16384
print("R04 IsOS candidate preflight fixtures: 18 cases and closed result examples pass")
