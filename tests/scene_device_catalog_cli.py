#!/usr/bin/env python3
"""P02 clean-home IsOS admission replay with the real MuJoCo 0.1.1 bundle.

Build IsOS and EOS first, then run this file from any working directory. The
bundle builder uses the already built simulator executable and installed
MuJoCo 3.16.0 root; this suite does not benchmark simulator throughput.
"""

import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
MUJOCO_REPO = Path(os.environ.get("ISOS_P02_MUJOCO_REPO", ROOT.parent / "isos-mujoco-sim"))
MUJOCO_ROOT = Path(os.environ.get(
    "MUJOCO_ROOT", "/home/ashex/mujoco-3.16.0-linux-x86_64/mujoco-3.16.0"))
ISOS = Path(os.environ.get("ISOS_BIN", ROOT / "target/debug/isos"))
EOS_BIN = Path(os.environ.get("AETHER_BIN_DIR", ROOT.parent / "Island-eos/target/debug"))
UI_DIST = Path(os.environ.get("ISOSUI_DIST_DIR", ROOT.parent / "IsosUI/dist/local"))
PIN = {"id": "island/mujoco-sim", "version": "0.1.1"}
SIMULATOR = "/cores/eos/layers/process/groups/mujoco/nodes/simulator"
DEVICE = "/cores/eos/layers/hardware/groups/manipulator/nodes/arm/adapter/settings/device_id"
FIRST_PIN = "/cores/eos/layers/hardware/groups/base/nodes/drive/adapter/package"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


class Home:
    def __init__(self, root, bundle, *, mujoco_root=MUJOCO_ROOT):
        self.root = Path(root)
        self.home = self.root / "home"
        self.bundle = self.root / "bundle"
        shutil.copytree(bundle, self.bundle)
        self.graph_path = self.home / "config/deployments/default.nodes.json"
        self.port = free_port()
        self.env = dict(os.environ, ISOS_HOME=str(self.home),
                        ISOS_HTTP_PORT=str(self.port), ISOSUI_PORT=str(free_port()),
                        ISOS_AETHER_BIN_DIR=str(EOS_BIN), ISOSUI_DIST_DIR=str(UI_DIST),
                        MUJOCO_ROOT=str(mujoco_root))
        self.server = None
        self.server_log = None
        self.running = None
        self.run_log = None

    def command(self, *args, success=True, timeout=60):
        result = subprocess.run([str(ISOS), *map(str, args)], env=self.env,
                                capture_output=True, text=True, timeout=timeout)
        print("$ isos", *args, "=>", result.returncode, flush=True)
        if result.stdout:
            print(result.stdout.rstrip(), flush=True)
        if result.stderr:
            print(result.stderr.rstrip(), flush=True)
        if success and result.returncode:
            raise AssertionError(f"IsOS command failed: {args}")
        return result

    def api(self, path, method="GET", data=None):
        request = Request(f"http://127.0.0.1:{self.port}/api/isos/v1/{path}",
                          data=None if data is None else json.dumps(data).encode(),
                          method=method,
                          headers={"Origin": f"http://127.0.0.1:{self.port}",
                                   "Content-Type": "application/json"})
        try:
            with urlopen(request, timeout=60) as response:
                return response.status, json.load(response)
        except HTTPError as error:
            return error.code, json.load(error)

    def start_server(self):
        self.server_log = (self.root / "serve.log").open("w")
        self.server = subprocess.Popen([str(ISOS), "serve"], env=self.env,
                                       stdout=self.server_log,
                                       stderr=subprocess.STDOUT, text=True)
        for _ in range(100):
            if self.server.poll() is not None:
                raise AssertionError(f"IsOS serve exited {self.server.returncode}")
            try:
                status, body = self.api("process-status")
                if status == 200 and body["ok"]:
                    return
            except OSError:
                pass
            time.sleep(0.05)
        raise AssertionError("IsOS serve did not become ready")

    def close(self):
        if self.running is not None:
            self.running.terminate()
            try:
                self.running.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.running.kill()
                self.running.wait(timeout=10)
        if self.server is not None:
            self.server.terminate()
            try:
                self.server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.server.kill()
                self.server.wait(timeout=10)
        if self.server_log is not None:
            self.server_log.close()
        if self.run_log is not None:
            self.run_log.close()

    def start_deployment(self):
        self.run_log = (self.root / "run.log").open("w")
        self.running = subprocess.Popen([str(ISOS), "run"], env=self.env,
                                        stdout=self.run_log,
                                        stderr=subprocess.STDOUT, text=True)
        for _ in range(300):
            if self.running.poll() is not None:
                self.run_log.flush()
                raise AssertionError(f"IsOS run exited {self.running.returncode}: "
                                     f"{(self.root / 'run.log').read_text()[-3000:]}")
            status = self.status()
            if status["state"] == "running":
                return status
            time.sleep(0.1)
        raise AssertionError("real MuJoCo deployment did not become running")

    def init_link_preview(self, *, scene="pick-place"):
        self.command("init", "--mode", "dev")
        initial = self.graph_path.read_bytes()
        self.command("pkg", "link", PIN["id"], PIN["version"], self.bundle,
                     "--manifest", self.bundle / "release.json")
        self.assert_package_list()
        sets = json.loads(self.command("pkg", "node-sets").stdout)["sets"]
        selected = [entry for entry in sets if entry["id"] == "sim-hardware"
                    and entry["package"] == PIN]
        if len(selected) != 1:
            raise AssertionError(f"expected one verified sim-hardware set, found {selected}")
        if self.graph_path.read_bytes() != initial:
            raise AssertionError("link or node-set listing changed saved graph")
        values = self.root / "values.json"
        values.write_text(json.dumps({"scene": scene,
                                      "socket": str(self.home / "run/default-mujoco.sock")}))
        preview = json.loads(self.command(
            "pkg", "preview-set", PIN["id"], PIN["version"], "sim-hardware",
            self.graph_path, "--values", values).stdout)
        if self.graph_path.read_bytes() != initial:
            raise AssertionError("preview changed saved graph")
        return initial, preview["graph"]

    def assert_package_list(self):
        listed = json.loads(self.command("pkg", "list", "--json").stdout)
        exact = [item for item in listed if item["id"] == PIN["id"]
                 and item["version"] == PIN["version"]]
        if len(exact) != 1 or exact[0]["status"] != "verified" or not exact[0]["available"]:
            raise AssertionError(f"expected verified exact pin, found {exact}")

    def status(self):
        status, body = self.api("process-status")
        if status != 200 or not body["ok"]:
            raise AssertionError(body)
        return body["data"]


class SceneDeviceCatalogReplay(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        required = [ISOS, EOS_BIN / "aether-profile", EOS_BIN / "aether-runtime",
                    EOS_BIN / "aether-http-bridge", EOS_BIN / "aether-config-server",
                    MUJOCO_REPO / "build/eos-mujoco-sim",
                    MUJOCO_REPO / "tools/build-backend-bundle.py",
                    MUJOCO_ROOT / "lib/libmujoco.so.3.16.0", UI_DIST / "index.html"]
        for path in required:
            if not path.is_file():
                raise AssertionError(f"missing P02 prerequisite: {path}")
        cls.bundle_temp = tempfile.TemporaryDirectory(prefix="isos-p02-bundle-")
        cls.bundle = Path(cls.bundle_temp.name) / "bundle"
        result = subprocess.run(
            ["python3", str(MUJOCO_REPO / "tools/build-backend-bundle.py"),
             "--build-dir", str(MUJOCO_REPO / "build"), "--output", str(cls.bundle)],
            capture_output=True, text=True, timeout=120)
        print("$ build-backend-bundle.py =>", result.returncode, flush=True)
        print(result.stdout, result.stderr, flush=True)
        if result.returncode:
            raise AssertionError("real MuJoCo 0.1.1 bundle build failed")
        descriptor = json.loads((cls.bundle / "backend-v2.json").read_text())
        if descriptor["catalog_version"] != "0.1.0":
            raise AssertionError("bundle lacks target P02 catalog")
        if [scene["id"] for scene in descriptor["scenes"]] != ["pick-place", "navigation"]:
            raise AssertionError("bundle scene IDs changed")

    @classmethod
    def tearDownClass(cls):
        cls.bundle_temp.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="isos-p02-home-")
        self.case = Home(self.temp.name, self.bundle)

    def tearDown(self):
        self.case.close()
        self.temp.cleanup()

    def assert_diagnostic(self, graph, code, path, stage):
        status, response = self.case.api("node-graph/validate", "POST",
                                         {"schema_version": 1, "graph": graph})
        self.assertEqual(status, 200, response)
        self.assertTrue(response["ok"], response)
        self.assertFalse(response["data"]["valid"], response)
        self.assertEqual({key: response["data"]["diagnostics"][0][key]
                          for key in ("code", "path", "stage")},
                         {"code": code, "path": path, "stage": stage})
        self.assertNotIn(str(self.case.root), json.dumps(response))

    def assert_apply_rejected_without_change(self, graph, code, path, stage):
        before_bytes = self.case.graph_path.read_bytes()
        before_status = self.case.status()
        status, response = self.case.api("node-graph", "PUT", {
            "schema_version": 1, "revision": digest(before_bytes), "graph": graph})
        self.assertEqual(status, 422, response)
        self.assertFalse(response["ok"], response)
        self.assertEqual({key: response["data"]["diagnostics"][0][key]
                          for key in ("code", "path", "stage")},
                         {"code": code, "path": path, "stage": stage})
        self.assertEqual(self.case.graph_path.read_bytes(), before_bytes)
        after_status = self.case.status()
        for field in ("saved_revision", "active_revision", "state", "processes"):
            self.assertEqual(after_status[field], before_status[field], field)

    def test_real_bundle_link_catalog_and_p01_seal_before_profile_admission(self):
        self.case.command("init", "--mode", "dev")
        initial = self.case.graph_path.read_bytes()
        self.case.command("pkg", "link", PIN["id"], PIN["version"], self.case.bundle,
                          "--manifest", self.case.bundle / "release.json")
        self.case.assert_package_list()
        sets = json.loads(self.case.command("pkg", "node-sets").stdout)["sets"]
        matched = [item for item in sets if item["id"] == "sim-hardware"
                   and item["package"] == PIN]
        self.assertEqual(len(matched), 1)
        self.assertEqual(len(matched[0]["nodes"]), 7)
        descriptor = json.loads((self.case.bundle / "backend-v2.json").read_text())
        self.assertEqual([scene["id"] for scene in descriptor["scenes"]],
                         ["pick-place", "navigation"])
        self.assertTrue(all(scene["description"] for scene in descriptor["scenes"]))
        self.assertTrue(all(device["description"] for device in descriptor["device_catalog"]))
        self.assertEqual(self.case.graph_path.read_bytes(), initial)

        graph = json.loads(initial)
        graph["cores"]["eos"]["layers"]["process"] = {"groups": {"mujoco": {"nodes": {
            "simulator": {"id": "simulator", "node_type": "eos.process.mujoco@v0.1",
                          "package": PIN, "settings": {"scene": "pick-place",
                                                     "socket": str(self.case.home / "run/default-mujoco.sock")}}
        }}}}
        scene_file = self.case.bundle / "assets/scene.xml"
        scene_file.write_bytes(scene_file.read_bytes() + b"\n")
        self.case.start_server()
        self.assert_diagnostic(graph, "package_changed", SIMULATOR + "/package", "payload")
        self.assert_apply_rejected_without_change(graph, "package_changed",
                                                  SIMULATOR + "/package", "payload")
        self.assertEqual(self.case.graph_path.read_bytes(), initial)

    def test_partial_catalog_link_rejected_without_receipt(self):
        self.case.command("init", "--mode", "dev")
        initial = self.case.graph_path.read_bytes()
        descriptor_file = self.case.bundle / "backend-v2.json"
        descriptor = json.loads(descriptor_file.read_text())
        descriptor.pop("device_catalog")
        descriptor_file.write_text(json.dumps(descriptor))
        rejected = self.case.command("pkg", "link", PIN["id"], PIN["version"],
                                     self.case.bundle, "--manifest",
                                     self.case.bundle / "release.json", success=False)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("Reinstall or relink island/mujoco-sim@0.1.1", rejected.stderr)
        self.case.start_server()
        status, response = self.case.api("packages/link", "POST", {
            **PIN, "path": str(self.case.bundle),
            "manifest": str(self.case.bundle / "release.json")})
        self.assertEqual(status, 422, response)
        self.assertEqual({key: response["data"]["diagnostics"][0][key]
                          for key in ("code", "path", "stage")},
                         {"code": "mujoco_catalog_incomplete", "path": "",
                          "stage": "descriptor"})
        self.assertNotIn(str(self.case.root), json.dumps(response))
        self.assertFalse((self.case.home / "data/packages/island/mujoco-sim/0.1.1").exists())
        self.assertEqual(self.case.graph_path.read_bytes(), initial)

    def test_verified_catalog_preview_apply_and_negative_admission(self):
        initial, graph = self.case.init_link_preview()
        self.assertEqual(graph["version"], "0.1.0")
        simulator = graph["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]
        self.assertEqual(simulator["node_type"], "eos.process.mujoco@v0.1")
        self.assertEqual(simulator["package"], PIN)
        self.assertEqual(simulator["settings"]["scene"], "pick-place")
        hardware = graph["cores"]["eos"]["layers"]["hardware"]["groups"]
        for group, node, kind, device_id in (
            ("camera", "front", "camera", "cam-0"),
            ("base", "drive", "base", "base-0"),
            ("manipulator", "arm", "manipulator", "arm-0"),
        ):
            item = hardware[group]["nodes"][node]
            self.assertEqual(item["node_type"], f"eos.hardware.{kind}@v0.1")
            self.assertEqual(item["adapter"]["package"], PIN)
            self.assertEqual(item["adapter"]["settings"]["device_id"], device_id)
        self.case.start_server()
        status, validated = self.case.api("node-graph/validate", "POST",
                                          {"schema_version": 1, "graph": graph})
        self.assertEqual(status, 200, validated)
        self.assertTrue(validated["data"]["valid"], validated)
        candidate = self.case.root / "candidate.nodes.json"
        candidate.write_text(json.dumps(graph, indent=2))
        self.case.command("config", "validate", candidate)
        self.assertEqual(self.case.graph_path.read_bytes(), initial)
        self.case.command("config", "apply", candidate,
                          "--revision", digest(initial))
        applied = self.case.graph_path.read_bytes()
        self.assertNotEqual(applied, initial)
        self.assertEqual(json.loads(applied), graph)
        self.assertEqual(self.case.status()["saved_revision"], digest(applied))
        running = self.case.start_deployment()
        self.assertEqual(running["active_revision"], digest(applied))
        self.assertTrue(running["processes"])

        wrong_scene = copy.deepcopy(graph)
        wrong_scene["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]["scene"] = "unknown-scene"
        self.assert_diagnostic(wrong_scene, "mujoco_scene_unavailable",
                               SIMULATOR + "/settings/scene", "admission")
        self.assert_apply_rejected_without_change(wrong_scene, "mujoco_scene_unavailable",
                                                  SIMULATOR + "/settings/scene", "admission")

        wrong_device = copy.deepcopy(graph)
        wrong_device["cores"]["eos"]["layers"]["hardware"]["groups"]["manipulator"]["nodes"]["arm"]["adapter"]["settings"]["device_id"] = "other-arm"
        self.assert_diagnostic(wrong_device, "mujoco_device_unavailable", DEVICE, "admission")
        self.assert_apply_rejected_without_change(wrong_device, "mujoco_device_unavailable",
                                                  DEVICE, "admission")

        scene_file = self.case.bundle / "assets/scene.xml"
        scene_file.write_bytes(scene_file.read_bytes() + b"\n")
        self.assert_diagnostic(graph, "package_changed", FIRST_PIN, "payload")
        self.assert_apply_rejected_without_change(graph, "package_changed",
                                                  FIRST_PIN, "payload")

    def test_missing_real_library_blocks_before_graph_publication(self):
        initial, graph = self.case.init_link_preview()
        self.case.env["MUJOCO_ROOT"] = str(Path(self.temp.name) / "missing-mujoco")
        self.case.start_server()
        self.assert_diagnostic(graph, "mujoco_library_unavailable",
                               SIMULATOR + "/package", "admission")
        self.assert_apply_rejected_without_change(graph, "mujoco_library_unavailable",
                                                  SIMULATOR + "/package", "admission")
        self.assertEqual(self.case.graph_path.read_bytes(), initial)


if __name__ == "__main__":
    unittest.main(verbosity=2)
