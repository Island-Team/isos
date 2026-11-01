#!/usr/bin/env python3
"""P05 selected-reader replay against the real EOS build and isolated EGIA fixture."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from generated_config_preflight_cli import CleanHome, EOS_BIN_DIR, digest


class ReaderFeaturesReplay(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="isos-p05-")
        self.case = CleanHome(self.temp.name)
        self.graph = self.case.init()

    def tearDown(self):
        self.case.close()
        self.temp.cleanup()

    def test_real_selected_eos_readers_validate_and_apply(self):
        self.case.apply(self.graph)
        revision, _, leaf = self.case.selected()
        self.assertEqual(json.loads((leaf / "manifest.json").read_text())["revision"], revision)
        check = self.case.command("version", "check", "--json")
        eos = next(core for core in json.loads(check.stdout)["cores"] if core["core"] == "eos")
        self.assertTrue(eos["ready"])
        self.assertEqual(eos["diagnostics"], [])

    def test_selected_runtime_missing_claim_fails_before_apply(self):
        selected = self.case.root / "selected-eos"
        selected.mkdir()
        for name in ("aether-profile", "aether-runtime", "aether-http-bridge",
                     "aether-config-server"):
            shutil.copy2(EOS_BIN_DIR / name, selected / name)
        runtime = selected / "aether-runtime"
        original = EOS_BIN_DIR / "aether-runtime"
        capabilities = json.loads(subprocess.check_output([str(original), "--capabilities"]))
        capabilities["features"] = [feature for feature in capabilities["features"]
                                    if feature != "node-types"]
        runtime.write_text("#!/usr/bin/env python3\nimport os,sys\n"
                           "if sys.argv[1:] == ['--capabilities']:\n"
                           f" print({json.dumps(json.dumps(capabilities))}); sys.exit(0)\n"
                           f"os.execv({str(original)!r}, [{str(original)!r}, *sys.argv[1:]])\n")
        runtime.chmod(0o700)
        self.case.env["ISOS_AETHER_BIN_DIR"] = str(selected)
        before = self.case.graph_path.read_bytes()
        self.case.start_server()
        status, reply = self.case.http_apply(self.graph)
        self.assertEqual(status, 422, reply)
        diagnostic = reply["data"]["diagnostics"][0]
        self.assertEqual((diagnostic["code"], diagnostic["path"], diagnostic["stage"]),
                         ("reader_feature_unsupported", "/version", "feature"))
        self.assertEqual(diagnostic["reader"]["binary"], "aether-runtime")
        self.assertEqual(diagnostic["reader"]["requested"]["feature"], "node-types")
        self.assertEqual(self.case.graph_path.read_bytes(), before)

    def test_bridge_requires_transport_feature_without_graph_arrays(self):
        selected = self.case.root / "selected-eos"
        selected.mkdir()
        for name in ("aether-profile", "aether-runtime", "aether-http-bridge",
                     "aether-config-server"):
            shutil.copy2(EOS_BIN_DIR / name, selected / name)
        bridge = selected / "aether-http-bridge"
        original = EOS_BIN_DIR / "aether-http-bridge"
        capabilities = json.loads(subprocess.check_output([str(original), "--capabilities"]))
        capabilities["features"] = [feature for feature in capabilities["features"]
                                    if feature != "vision-ws"]
        bridge.write_text("#!/usr/bin/env python3\nimport os,sys\n"
                          "if sys.argv[1:] == ['--capabilities']:\n"
                          f" print({json.dumps(json.dumps(capabilities))}); sys.exit(0)\n"
                          f"os.execv({str(original)!r}, [{str(original)!r}, *sys.argv[1:]])\n")
        bridge.chmod(0o700)
        self.case.env["ISOS_AETHER_BIN_DIR"] = str(selected)
        self.case.start_server()
        status, reply = self.case.http_apply(self.graph)
        self.assertEqual(status, 422, reply)
        diagnostic = reply["data"]["diagnostics"][0]
        self.assertEqual(diagnostic["code"], "reader_feature_unsupported")
        self.assertEqual(diagnostic["reader"]["binary"], "aether-http-bridge")
        self.assertEqual(diagnostic["reader"]["requested"]["feature"], "vision-ws")
        self.assertEqual(diagnostic["reader"]["supported"]["graph_versions"], [])
        self.assertEqual(diagnostic["path"],
                         "/cores/isos/layers/process/groups/component/nodes/bridge/node_type")

    def test_runtime_claim_change_rejects_restart_without_stopping_children(self):
        selected = self.case.root / "selected-eos"
        selected.mkdir()
        for name in ("aether-profile", "aether-runtime", "aether-http-bridge",
                     "aether-config-server"):
            shutil.copy2(EOS_BIN_DIR / name, selected / name)
        runtime = selected / "aether-runtime"
        original = EOS_BIN_DIR / "aether-runtime"
        capabilities = json.loads(subprocess.check_output([str(original), "--capabilities"]))
        weaker = dict(capabilities, features=[feature for feature in capabilities["features"]
                                             if feature != "node-types"])
        toggle = self.case.root / "drop-feature"
        runtime.write_text("#!/usr/bin/env python3\nimport os,sys\n"
                           "if sys.argv[1:] == ['--capabilities']:\n"
                           f" print({json.dumps(json.dumps(weaker))} if os.path.exists({str(toggle)!r}) "
                           f"else {json.dumps(json.dumps(capabilities))}); sys.exit(0)\n"
                           f"os.execv({str(original)!r}, [{str(original)!r}, *sys.argv[1:]])\n")
        runtime.chmod(0o700)
        self.case.env["ISOS_AETHER_BIN_DIR"] = str(selected)
        self.case.apply(self.graph)
        self.case.start_server()
        status, reply = self.case.control("run")
        self.assertEqual(status, 200, reply)
        status, reply = self.case.api("process-status")
        self.assertEqual(status, 200, reply)
        pids = {item["id"]: item["pid"] for item in reply["data"]["processes"]}
        self.assertTrue(pids)
        toggle.write_text("omit node-types\n")
        status, reply = self.case.control("restart")
        self.assertEqual(status, 422, reply)
        diagnostic = reply["data"]["diagnostics"][0]
        self.assertEqual((diagnostic["code"], diagnostic["path"], diagnostic["stage"]),
                         ("reader_feature_unsupported", "/version", "feature"))
        status, after = self.case.api("process-status")
        self.assertEqual(status, 200, after)
        self.assertEqual({item["id"]: item["pid"] for item in after["data"]["processes"]}, pids)

    def test_egia_module_change_invalidates_selected_generation(self):
        self.case.seed_egia_distribution()
        nodes = self.graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]
        nodes["egia"] = {
            "id": "egia", "node_type": "isos.process.component@v0.1",
            "label": "EGIA agent", "description": "P05 EGIA chain fixture",
            "capabilities": ["process"],
            "settings": {"role": "egia", "args": [], "depends_on": [
                "cores/isos/layers/process/groups/component/nodes/eos"]},
        }
        self.case.apply(self.graph)
        _, _, leaf = self.case.selected()
        manifest = json.loads((leaf / "manifest.json").read_text())
        self.assertEqual(manifest["egia_chain"]["mode"], "source")
        self.assertEqual(manifest["egia_chain"]["binding"]["PYTHONPATH"], "")
        inventory = {key: value for key, value in manifest.items() if key != "generation"}
        canonical = json.dumps(inventory, sort_keys=True, separators=(",", ":"),
                               ensure_ascii=False).encode()
        self.assertEqual(digest(canonical), manifest["generation"])
        module = self.case.root / "egia-install/src/egia/__init__.py"
        module.write_text("# changed selected module\n")
        self.case.start_server()
        status, reply = self.case.control("run")
        self.assertEqual(status, 422, reply)
        self.assertEqual(reply["data"]["diagnostics"][0]["code"],
                         "generated_manifest_mismatch")

    def test_egia_setup_must_claim_all_four_legacy_types(self):
        self.case.seed_egia_distribution()
        nodes = self.graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]
        nodes["egia"] = {
            "id": "egia", "node_type": "isos.process.component@v0.1",
            "label": "EGIA agent", "description": "P05 EGIA type fixture",
            "capabilities": ["process"],
            "settings": {"role": "egia", "args": [], "depends_on": [
                "cores/isos/layers/process/groups/component/nodes/eos"]},
        }
        adapter = self.case.root / "egia-install/bin/egia-config"
        source = adapter.read_text()
        adapter.write_text(source.replace(', "egia.common.source@v1.0"', ''))
        self.case.start_server()
        status, reply = self.case.http_apply(self.graph)
        self.assertEqual(status, 422, reply)
        diagnostic = reply["data"]["diagnostics"][0]
        self.assertEqual((diagnostic["code"], diagnostic["path"], diagnostic["stage"]),
                         ("reader_identity_invalid", "", "reader"))
        self.assertEqual(diagnostic["reader"]["binary"], "egia-config")

    def test_real_egia_readers_reach_owner_render(self):
        root = Path(os.environ.get("ISOS_REAL_EGIA_ROOT", ""))
        if not root.is_absolute() or not (root / "bin/egia-config").is_file():
            self.skipTest("set ISOS_REAL_EGIA_ROOT to a built EGIA checkout")
        self.case.env.update({
            "ISOS_EGIA_BIN_DIR": str(root / "bin"),
            "ISOS_EGIA_DEFAULTS_DIR": str(root / "etc/egia"),
            "ISOS_EGIA_FACTORY_DEFAULTS_DIR": str(root / "share/defaults/egia"),
            "EGIA_PYTHON": str(root / ".venv/bin/python"),
            "ISLAND_PREFIX": str(root),
        })
        nodes = self.graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]
        nodes["egia"] = {
            "id": "egia", "node_type": "isos.process.component@v0.1",
            "label": "EGIA agent", "description": "Real EGIA reader fixture",
            "capabilities": ["process"],
            "settings": {"role": "egia", "args": [], "depends_on": [
                "cores/isos/layers/process/groups/component/nodes/eos"]},
        }
        candidate = self.case.root / "real-egia.nodes.json"
        candidate.write_text(json.dumps(self.graph))
        result = self.case.command("config", "validate", candidate, success=False)
        self.assertIn('"code":"generated_owner_rejected"', result.stderr)
        self.assertNotIn('"code":"reader_', result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
