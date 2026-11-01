#!/usr/bin/env python3
"""P03 exact-pin options and stale Apply replay against a real MuJoCo bundle."""

import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import scene_device_catalog_cli as p02

Home = p02.Home
digest = p02.digest
SIMULATOR = p02.SIMULATOR
DEVICE = p02.DEVICE
EOS_BIN = p02.EOS_BIN


ROOT = Path(__file__).resolve().parents[1]
EXPECTED = json.loads((ROOT / "tests/fixtures/node_options/positive-response.json").read_text())


class NodeOptionsReplay(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        p02.SceneDeviceCatalogReplay.setUpClass()
        cls.bundle = p02.SceneDeviceCatalogReplay.bundle

    @classmethod
    def tearDownClass(cls):
        p02.SceneDeviceCatalogReplay.tearDownClass()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="isos-p03-home-")
        self.case = Home(self.temp.name, self.bundle)

    def tearDown(self):
        self.case.close()
        self.temp.cleanup()

    def preview(self):
        initial, graph = self.case.init_link_preview()
        self.case.start_server()
        return initial, graph

    def options(self, graph):
        status, body = self.case.api("node-options", "POST",
                                     {"schema_version": 1, "graph": graph})
        self.assertEqual(status, 200, body)
        self.assertTrue(body["ok"], body)
        return body["data"]

    def assert_rejected(self, graph, code, path, stage):
        original = self.case.graph_path.read_bytes()
        old_status = self.case.status()
        status, body = self.case.api("node-graph", "PUT", {
            "schema_version": 1, "revision": digest(original), "graph": graph})
        self.assertEqual(status, 422, body)
        self.assertIn("stage", body["data"]["diagnostics"][0], body)
        self.assertEqual({key: body["data"]["diagnostics"][0][key]
                          for key in ("code", "path", "stage")},
                         {"code": code, "path": path, "stage": stage})
        self.assertEqual(self.case.graph_path.read_bytes(), original)
        new_status = self.case.status()
        for key in ("saved_revision", "active_revision", "state", "processes"):
            self.assertEqual(new_status[key], old_status[key])

    def assert_invalid(self, graph, code, path, stage):
        status, body = self.case.api("node-graph/validate", "POST", {
            "schema_version": 1, "graph": graph})
        self.assertEqual(status, 200, body)
        self.assertFalse(body["data"]["valid"], body)
        self.assertEqual({key: body["data"]["diagnostics"][0][key]
                          for key in ("code", "path", "stage")},
                         {"code": code, "path": path, "stage": stage})

    def test_real_catalog_rows_get_revision_and_no_write(self):
        initial, graph = self.preview()
        data = self.options(graph)
        self.assertEqual({key: data[key] for key in
                          ("setting_options", "graph_revision", "read_only")},
                         {key: EXPECTED["expected"]["data"][key] for key in
                          ("setting_options", "graph_revision", "read_only")})
        for key in ("adapter_families", "primitive_bindings", "targets"):
            self.assertIn(key, data)
        status, saved = self.case.api("node-graph")
        self.assertEqual(status, 200, saved)
        status, current = self.case.api("node-options")
        self.assertEqual(status, 200, current)
        self.assertEqual(current["data"]["graph_revision"], saved["data"]["revision"])
        self.assertEqual(current["data"]["setting_options"], [])
        self.assertEqual(self.case.graph_path.read_bytes(), initial)
        self.case.api("node-graph", "PUT", {"schema_version": 1,
            "revision": digest(initial), "graph": graph})
        status, saved = self.case.api("node-graph")
        status, current = self.case.api("node-options")
        self.assertEqual(current["data"]["graph_revision"], saved["data"]["revision"])
        self.assertEqual(current["data"]["setting_options"], data["setting_options"])

    def test_missing_pin_and_synthetic_selected_scene(self):
        _, graph = self.preview()
        original_graph = copy.deepcopy(graph)
        graph["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["package"]["version"] = "0.1.2"
        data = self.options(graph)
        scene = next(row for row in data["setting_options"] if row["setting"] == "scene")
        self.assertEqual(scene["options"][0]["value"], "pick-place")
        self.assertEqual(scene["options"][0]["reason_code"], "package_missing_file")
        self.assert_rejected(graph, "package_missing_file", SIMULATOR + "/package", "manifest")
        scene_file = self.case.bundle / "assets/scene.xml"
        scene_file.write_bytes(scene_file.read_bytes() + b"\n")
        data = self.options(original_graph)
        scene = next(row for row in data["setting_options"] if row["setting"] == "scene")
        self.assertEqual(scene["options"][0]["reason_code"], "package_changed")
        self.assert_rejected(original_graph, "package_changed", SIMULATOR + "/package", "payload")

    def test_missing_library_marks_verified_rows_unavailable(self):
        _, graph = self.case.init_link_preview()
        self.case.env["MUJOCO_ROOT"] = str(self.case.root / "missing-library")
        self.case.start_server()
        data = self.options(graph)
        for group in data["setting_options"]:
            self.assertTrue(group["options"], group)
            self.assertTrue(all(not option["available"] and
                option["reason_code"] == "mujoco_library_unavailable"
                for option in group["options"]), group)
        self.assert_rejected(graph, "mujoco_library_unavailable", SIMULATOR + "/package", "admission")

    def test_scene_change_recomputes_device_membership(self):
        descriptor_path = self.case.bundle / "backend-v2.json"
        descriptor = json.loads(descriptor_path.read_text())
        arm = next(item for item in descriptor["device_catalog"] if item["kind"] == "manipulator")
        arm["scene_ids"] = ["pick-place"]
        descriptor_path.write_text(json.dumps(descriptor))
        _, graph = self.preview()
        graph["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]["scene"] = "navigation"
        data = self.options(graph)
        arm_row = next(row for row in data["setting_options"] if row["node_path"] == "eos/hardware/manipulator/arm" and row["setting"] == "adapter.settings.device_id")
        self.assertEqual(arm_row["options"][0]["value"], "arm-0")
        self.assertFalse(arm_row["options"][0]["available"])
        self.assertEqual(arm_row["options"][0]["reason_code"], "mujoco_device_unavailable")
        camera_row = next(row for row in data["setting_options"] if row["node_path"] == "eos/hardware/camera/front" and row["setting"] == "adapter.settings.device_id")
        self.assertTrue(camera_row["options"][0]["available"])
        self.assert_rejected(graph, "mujoco_device_unavailable", DEVICE, "admission")

    def test_restart_rechecks_pin_before_replacing_active_simulator(self):
        original, graph = self.preview()
        status, applied = self.case.api("node-graph", "PUT", {
            "schema_version": 1, "revision": digest(original), "graph": graph})
        self.assertEqual(status, 200, applied)
        saved = self.case.graph_path.read_bytes()
        status, started = self.case.api("process-control", "POST", {
            "schema_version": 1, "operation": "run", "revision": digest(saved)})
        self.assertEqual(status, 200, started)
        before = self.case.status()
        self.assertEqual(before["state"], "running")
        pids = {process["id"]: process["pid"] for process in before["processes"]}
        self.assertTrue(pids)
        scene_file = self.case.bundle / "assets/scene.xml"
        scene_file.write_bytes(scene_file.read_bytes() + b"\n")
        status, rejected = self.case.api("process-control", "POST", {
            "schema_version": 1, "operation": "restart", "revision": digest(saved)})
        self.assertEqual(status, 422, rejected)
        self.assertEqual({key: rejected["data"]["diagnostics"][0][key]
                          for key in ("code", "path", "stage")},
                         {"code": "package_changed", "path": SIMULATOR + "/package", "stage": "payload"})
        after = self.case.status()
        self.assertEqual(self.case.graph_path.read_bytes(), saved)
        self.assertEqual(after["active_revision"], before["active_revision"])
        self.assertEqual({process["id"]: process["pid"] for process in after["processes"]}, pids)

    def test_second_verified_release_cannot_change_exact_process_options(self):
        initial, graph = self.case.init_link_preview()
        second = self.case.root / "bundle-second"
        shutil.copytree(self.case.bundle, second)
        descriptor = json.loads((second / "backend-v2.json").read_text())
        descriptor["package"]["version"] = "0.1.2"
        (second / "backend-v2.json").write_text(json.dumps(descriptor))
        release = json.loads((second / "release.json").read_text())
        release["version"] = "0.1.2"
        release["release_url"] = release["release_url"].replace("v0.1.1", "v0.1.2")
        for asset in release["assets"]:
            asset["name"] = asset["name"].replace("v0.1.1", "v0.1.2")
            asset["url"] = asset["url"].replace("v0.1.1", "v0.1.2")
        for node in release["node_contributions"]["node_sets"][0]["nodes"]:
            adapter = node["profile"].get("adapter")
            if adapter and adapter.get("package", {}).get("id") == "island/mujoco-sim":
                adapter["package"]["version"] = "0.1.2"
        (second / "release.json").write_text(json.dumps(release))
        self.case.command("pkg", "link", "island/mujoco-sim", "0.1.2", second,
                          "--manifest", second / "release.json")
        self.case.start_server()
        baseline = self.options(graph)["setting_options"]
        self.assertEqual(baseline, EXPECTED["expected"]["data"]["setting_options"])
        self.assertEqual(self.case.graph_path.read_bytes(), initial)
        status, accepted = self.case.api("node-graph", "PUT", {
            "schema_version": 1, "revision": digest(initial), "graph": graph})
        self.assertEqual(status, 200, accepted)
        wrong_pin = copy.deepcopy(graph)
        wrong_pin["cores"]["eos"]["layers"]["hardware"]["groups"]["manipulator"]["nodes"]["arm"]["adapter"]["package"]["version"] = "0.1.2"
        data = self.options(wrong_pin)
        row = next(row for row in data["setting_options"] if row["node_path"] == "eos/hardware/manipulator/arm" and row["setting"] == "adapter.settings.device_id")
        self.assertEqual(row["options"][0]["reason_code"], "mujoco_device_unavailable")
        self.assert_invalid(wrong_pin, "mujoco_device_unavailable", DEVICE, "admission")
        self.assert_rejected(wrong_pin, "mujoco_device_unavailable", DEVICE, "admission")

    def test_uncompiled_eos_catalog_marks_rows_and_blocks_apply(self):
        _, graph = self.case.init_link_preview()
        stub = self.case.root / "eos-uncompiled"
        stub.mkdir()
        for executable in ("aether-runtime", "aether-http-bridge", "aether-config-server"):
            (stub / executable).symlink_to(EOS_BIN / executable)
        wrapper = stub / "aether-profile"
        wrapper.write_text("""#!/usr/bin/env python3
import json
import subprocess
import sys
result = subprocess.run([%s, *sys.argv[1:]], capture_output=True)
if result.returncode == 0 and sys.argv[1:2] == ['catalog']:
    value = json.loads(result.stdout)
    for family in value.get('adapter_families', []):
        if family.get('id') == 'mujoco-v2':
            family['compiled'] = False
            family['reason'] = 'no EOS adapter implementation'
    sys.stdout.write(json.dumps(value))
else:
    sys.stdout.buffer.write(result.stdout)
sys.stderr.buffer.write(result.stderr)
sys.exit(result.returncode)
""" % json.dumps(str(EOS_BIN / "aether-profile")))
        wrapper.chmod(0o700)
        self.case.env["ISOS_AETHER_BIN_DIR"] = str(stub)
        self.case.start_server()
        data = self.options(graph)
        row = next(row for row in data["setting_options"] if row["node_path"] == "eos/hardware/base/drive" and row["setting"] == "adapter.settings.device_id")
        self.assertEqual(row["options"][0]["reason_code"], "adapter_unavailable")
        self.assertFalse(row["options"][0]["available"])
        self.assert_invalid(graph, "adapter_unavailable",
                            "/cores/eos/layers/hardware/groups/base/nodes/drive/adapter/type",
                            "admission")
        self.assert_rejected(graph, "adapter_unavailable",
                             "/cores/eos/layers/hardware/groups/base/nodes/drive/adapter/type",
                             "admission")

    def test_unknown_scene_device_and_malformed_id(self):
        _, graph = self.preview()
        unknown = copy.deepcopy(graph)
        unknown["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]["scene"] = "warehouse"
        data = self.options(unknown)
        scene = next(row for row in data["setting_options"] if row["setting"] == "scene")
        self.assertEqual(scene["options"][-1]["reason_code"], "mujoco_scene_unavailable")
        self.assert_rejected(unknown, "mujoco_scene_unavailable", SIMULATOR + "/settings/scene", "admission")
        wrong = copy.deepcopy(graph)
        wrong["cores"]["eos"]["layers"]["hardware"]["groups"]["manipulator"]["nodes"]["arm"]["adapter"]["settings"]["device_id"] = "base-0"
        data = self.options(wrong)
        row = next(row for row in data["setting_options"] if row["node_path"] == "eos/hardware/manipulator/arm" and row["setting"] == "adapter.settings.device_id")
        self.assertEqual(row["options"][-1]["reason_code"], "mujoco_device_unavailable")
        self.assert_rejected(wrong, "mujoco_device_unavailable", DEVICE, "admission")
        unpaired = copy.deepcopy(graph)
        unpaired["cores"]["eos"]["layers"]["hardware"]["groups"]["manipulator"]["nodes"]["arm"]["adapter"]["settings"]["simulator_node"] = "other-arm"
        self.assert_rejected(unpaired, "mujoco_device_unavailable", DEVICE, "admission")
        unreferenced = copy.deepcopy(graph)
        unreferenced["cores"]["eos"]["layers"]["hardware"]["groups"]["manipulator"]["nodes"]["arm"]["references"] = []
        data = self.options(unreferenced)
        row = next(row for row in data["setting_options"] if row["node_path"] == "eos/hardware/manipulator/arm" and row["setting"] == "adapter.settings.device_id")
        self.assertEqual(row["options"][0]["reason_code"], "mujoco_device_unavailable")
        self.assert_rejected(unreferenced, "mujoco_device_unavailable", DEVICE, "admission")
        malformed = copy.deepcopy(graph)
        malformed["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]["scene"] = "/tmp/secret.xml"
        status, body = self.case.api("node-options", "POST", {"schema_version": 1, "graph": malformed})
        self.assertEqual(status, 422, body)
        self.assertEqual(body["data"]["diagnostics"][0]["code"], "node_option_invalid_id")
        self.assertNotIn("/tmp/secret.xml", json.dumps(body))
        self.assert_rejected(malformed, "node_option_invalid_id", SIMULATOR + "/settings/scene", "admission")
        malformed["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]["scene"] = None
        status, body = self.case.api("node-options", "POST", {"schema_version": 1, "graph": malformed})
        self.assertEqual(status, 422, body)
        self.assertEqual(body["data"]["diagnostics"][0]["code"], "node_option_invalid_id")
        self.assert_rejected(malformed, "node_option_invalid_id", SIMULATOR + "/settings/scene", "admission")

    def test_unknown_graph_identity_and_stale_revision(self):
        initial, graph = self.preview()
        future = copy.deepcopy(graph)
        future["version"] = "2.0.0"
        status, body = self.case.api("node-options", "POST", {"schema_version": 1, "graph": future})
        self.assertEqual(status, 422, body)
        self.assertEqual(body["data"]["diagnostics"][0]["code"], "unsupported_graph_version")
        unknown = copy.deepcopy(graph)
        unknown["cores"]["eos"]["layers"]["hardware"]["groups"]["camera"]["nodes"]["front"]["node_type"] = "eos.hardware.future@v0.1"
        status, body = self.case.api("node-options", "POST", {"schema_version": 1, "graph": unknown})
        self.assertEqual(status, 422, body)
        self.assertEqual(body["data"]["diagnostics"][0]["code"], "unsupported_node_type")
        self.case.api("node-graph", "PUT", {"schema_version": 1,
            "revision": digest(initial), "graph": graph})
        status, body = self.case.api("node-graph", "PUT", {"schema_version": 1,
            "revision": digest(initial), "graph": graph})
        self.assertEqual(status, 409, body)
        self.assertEqual(body["error"], "node graph revision changed; reload before applying")

    def test_get_missing_graph_and_applied_legacy_revision(self):
        self.case.command("init", "--mode", "dev")
        self.case.start_server()
        self.case.graph_path.unlink()
        status, graph = self.case.api("node-graph")
        self.assertEqual(status, 200, graph)
        status, options = self.case.api("node-options")
        self.assertEqual(status, 200, options)
        self.assertEqual(options["data"]["graph_revision"], "")
        self.assertFalse(options["data"]["read_only"])
        self.assertEqual(options["data"]["setting_options"], [])
        self.assertEqual(options["data"]["graph_revision"], graph["data"]["revision"])
        legacy = (ROOT / "tests/fixtures/graph_compat/legacy_1_0_0.json").read_bytes()
        self.case.graph_path.write_bytes(legacy)
        applied = self.case.home / "generated/default/core-setup/launch-inputs.json"
        applied.parent.mkdir(parents=True, exist_ok=True)
        applied.write_text("{}")
        status, graph = self.case.api("node-graph")
        self.assertEqual(status, 200, graph)
        status, options = self.case.api("node-options")
        self.assertEqual(status, 200, options)
        self.assertEqual(options["data"]["graph_revision"], digest(legacy))
        self.assertEqual(options["data"]["graph_revision"], graph["data"]["revision"])
        self.assertEqual(options["data"]["read_only"], graph["data"]["read_only"])
        self.assertEqual(options["data"]["setting_options"], [])
        self.assertEqual(self.case.graph_path.read_bytes(), legacy)


if __name__ == "__main__":
    unittest.main(verbosity=2)
