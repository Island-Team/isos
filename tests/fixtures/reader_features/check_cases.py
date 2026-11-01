#!/usr/bin/env python3
"""Check the frozen P05 design fixture without a runtime implementation."""

import json
from pathlib import Path


def main() -> None:
    root = Path(__file__).resolve().parents[3]
    fixture = json.loads((Path(__file__).parent / "cases.json").read_text())
    assert fixture["schema_version"] == 1
    assert fixture["slice"] == "P05"
    assert (root / fixture["baseline_graph"]).is_file()
    assert fixture["baseline_package"] == {
        "id": "island/mujoco-sim",
        "version": "0.1.1",
    }
    cases = fixture["cases"]
    assert len(cases) == 36
    ids = [case["id"] for case in cases]
    assert len(ids) == len(set(ids))
    operations = {"validate", "apply", "run", "restart", "version_check", "validate_apply_run"}
    outcomes = {"admit", "admit_read_only", "admit_reader_only", "reject", "suggest", "blocked", "first_failure"}
    codes = {
        "unsupported_graph_version", "unsupported_node_type", "package_invalid_sidecar",
        "package_changed", "generated_manifest_mismatch", "reader_identity_invalid",
        "reader_graph_unsupported", "reader_type_unsupported", "reader_feature_unsupported",
        "reader_binary_mismatch", "reader_package_pin_mismatch",
    }
    stages = {"graph", "package", "reader", "feature", "sidecar", "snapshot", "manifest"}
    for case in cases:
        assert case["id"] and case["operation"] in operations and case["mutation"]
        expected = case["expected"]
        assert expected["result"] in outcomes
        if expected["result"] == "reject":
            assert expected["code"] in codes and expected["stage"] in stages
            assert expected["path"] == "" or expected["path"].startswith("/")
            assert expected["saved_bytes_unchanged"] is True
            assert expected["stop_calls"] == expected["spawn_calls"] == 0
            if case["operation"] == "restart":
                assert expected["active_pids_unchanged"] is True
            if expected["code"] == "reader_feature_unsupported":
                assert expected["requested_feature"]
        else:
            assert "code" not in expected
    print(f"P05 fixture valid: {len(cases)} unique cases")


if __name__ == "__main__":
    main()
