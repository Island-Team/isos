#!/usr/bin/env python3
"""P01 clean-home package verification and graph admission replay.

Run after building IsOS and EOS, for example:
  python3 tests/package_verification_cli.py
The simulator is a tiny fixture; this suite does not claim MuJoCo physics coverage.
"""

import hashlib
import copy
import importlib.util
import json
import os
from pathlib import Path
import platform
import socket
import subprocess
import tarfile
import tempfile
import time
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
ISOS = Path(os.environ.get("ISOS_BIN", ROOT / "target/debug/isos"))
EOS_BIN = Path(os.environ.get("AETHER_BIN_DIR", ROOT.parent / "Island-eos/target/debug"))
CONTRIBUTIONS = ROOT.parent / "isos-mujoco-sim/tools/node_contributions.py"
PIN = {"id": "island/mujoco-sim", "version": "0.1.1"}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def contributions():
    spec = importlib.util.spec_from_file_location("p01_mujoco_contributions", CONTRIBUTIONS)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    catalog = module.contributions()
    # P02 will publish the new process identity. Include the frozen target in
    # this P01 fixture so legacy migration can verify its exact package pin.
    current = copy.deepcopy(catalog["node_types"][0])
    current["id"] = "eos.process.mujoco@v0.1"
    current["description"] = "Starts the verified simulator and publishes process health."
    for setting in current["settings"]:
        setting["description"] = f"Verified simulator {setting['name']} setting"
    catalog["node_types"].append(current)
    return catalog


class CleanHome:
    def __init__(self, root):
        self.root = Path(root)
        self.home = self.root / "home"
        self.bundle = self.root / "bundle"
        self.archive = self.root / "bundle.tar.gz"
        self.manifest = self.root / "release.json"
        self.graph_path = self.home / "config/deployments/default.nodes.json"
        self.port = free_port()
        self.server = None
        self.log = None
        library = self.root / "mujoco/lib"
        library.mkdir(parents=True)
        (library / "libmujoco.so.3.16.0").write_bytes(b"P01 test-only library")
        self.env = dict(os.environ, ISOS_HOME=str(self.home), MUJOCO_ROOT=str(library.parent),
                        ISOS_HTTP_PORT=str(self.port), ISOS_AETHER_BIN_DIR=str(EOS_BIN),
                        ISOSUI_PORT=str(free_port()))

    def run(self, *args, success=True, timeout=30):
        result = subprocess.run([str(ISOS), *map(str, args)], env=self.env,
                                text=True, capture_output=True, timeout=timeout)
        print("$", "isos", *args, "=>", result.returncode, flush=True)
        if result.stdout:
            print(result.stdout.rstrip(), flush=True)
        if result.stderr:
            print(result.stderr.rstrip(), flush=True)
        if success and result.returncode:
            raise AssertionError(f"isos {' '.join(map(str, args))} failed")
        return result

    def create_bundle(self):
        (self.bundle / "bin").mkdir(parents=True)
        (self.bundle / "assets").mkdir()
        executable = self.bundle / "bin/eos-mujoco-sim"
        executable.write_text("#!/bin/sh\nexit 0\n")
        executable.chmod(0o755)
        (self.bundle / "assets/scene.xml").write_text("<mujoco/>\n")
        descriptor = {
            "schema_version": "2.0.0", "package": PIN,
            "entrypoint": "bin/eos-mujoco-sim", "protocol": "mujoco-v2",
            "scenes": [{"id": "pick-place", "path": "assets/scene.xml"}],
            "devices": {"camera": "cam-0", "base": "base-0", "manipulator": "arm-0"},
            "runtime": {"mujoco_version": "3.16.0", "environment_variable": "MUJOCO_ROOT",
                        "library": "lib/libmujoco.so.3.16.0"},
        }
        (self.bundle / "backend-v2.json").write_text(json.dumps(descriptor))
        with tarfile.open(self.archive, "w:gz") as archive:
            for path in sorted(self.bundle.rglob("*")):
                archive.add(path, arcname=path.relative_to(self.bundle), recursive=False)
        manifest = {
            "schema_version": 4, **PIN, "kind": "backend", "source_commit": "a" * 40,
            "node_contributions": contributions(),
            "assets": [{"os": platform.system().lower(), "arch": platform.machine(),
                        "url": "https://github.com/Island-Team/isos-mujoco-sim/releases/download/v0.1.1/p01-test.tar.gz",
                        "size_bytes": self.archive.stat().st_size,
                        "sha256": sha256(self.archive.read_bytes())}],
        }
        self.manifest.write_text(json.dumps(manifest))
        return manifest

    def init(self):
        self.run("init", "--mode", "dev")
        return self.graph_path.read_bytes()

    def link(self):
        return self.run("pkg", "link", *PIN.values(), self.bundle, "--manifest", self.manifest)

    def install(self):
        return self.run("pkg", "install", "--manifest", self.manifest, "--asset", self.archive,
                        "--allow-unapproved")

    def stored(self):
        return self.home / "data/packages/island/mujoco-sim/0.1.1"

    def receipt(self):
        return json.loads((self.stored() / "receipt.json").read_text())

    def apply_bundle_graph(self, *, legacy=True):
        # The node-set composer is still 1.0.0-only pending P02/P03, so this
        # fixture declares the verified process directly under either reader.
        graph = json.loads(self.graph_path.read_text())
        saved_revision = sha256(self.graph_path.read_bytes())
        if legacy:
            graph["version"] = "1.0.0"
            graph["cores"]["eos"]["layers"]["dispatcher"]["groups"]["control"]["nodes"]["command"]["node_type"] = "eos.dispatcher.control@v1.0"
            components = graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]
            for node in components.values():
                node["node_type"] = "isos.process.component@v1.1"
        graph["cores"]["eos"]["layers"]["process"] = {"groups": {"mujoco": {"nodes": {
            "simulator": {
                "id": "simulator", "node_type": f"eos.process.mujoco@v{'1.0' if legacy else '0.1'}",
                "label": "MuJoCo simulator", "description": "Verified P01 test simulator",
                "capabilities": ["process"], "package": PIN,
                "settings": {"scene": "pick-place",
                             "socket": str(self.home / "run/default-mujoco.sock")},
            }
        }}}}
        components = graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]
        for name in ("bridge", "config"):
            endpoint = f"127.0.0.1:{free_port()}"
            components[name]["settings"]["args"][-1] = endpoint
            components[name]["settings"]["ready_tcp"] = endpoint
        candidate = self.graph_path if legacy else self.root / "new.nodes.json"
        candidate.write_text(json.dumps(graph, indent=2))
        self.run("config", "validate", candidate)
        if legacy:
            self.run("config", "apply", candidate)
        else:
            self.run("config", "apply", candidate, "--revision", saved_revision)
        return self.graph_path.read_bytes()

    def start_http(self):
        self.log = (self.root / "serve.log").open("w+")
        self.server = subprocess.Popen([str(ISOS), "serve"], env=self.env,
                                       stdout=self.log, stderr=subprocess.STDOUT, text=True)
        for _ in range(100):
            if self.server.poll() is not None:
                raise AssertionError(f"serve exited {self.server.returncode}")
            try:
                self.api("packages")
                return
            except (OSError, ValueError):
                time.sleep(0.05)
        raise AssertionError("IsOS HTTP did not become ready")

    def stop_http(self):
        if self.server is not None:
            self.server.terminate()
            try:
                self.server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.server.kill()
                self.server.wait(timeout=5)
        if self.log is not None:
            self.log.close()

    def api(self, path, method="GET", data=None):
        payload = None if data is None else json.dumps(data).encode()
        request = Request(f"http://127.0.0.1:{self.port}/api/isos/v1/{path}",
                          data=payload, method=method,
                          headers={"Origin": f"http://127.0.0.1:{self.port}",
                                   "Content-Type": "application/json"})
        try:
            with urlopen(request, timeout=5) as response:
                return response.status, json.load(response)
        except HTTPError as error:
            return error.code, json.load(error)


class PackageVerificationReplay(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for binary in [ISOS, EOS_BIN / "aether-profile", CONTRIBUTIONS]:
            if not binary.is_file():
                raise AssertionError(f"missing test prerequisite: {binary}")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="isos-p01-")
        self.case = CleanHome(self.temp.name)
        self.case.create_bundle()

    def tearDown(self):
        self.case.stop_http()
        self.temp.cleanup()

    def assert_receipt(self, source_kind):
        receipt = self.case.receipt()
        self.assertEqual({key: receipt[key] for key in PIN}, PIN)
        self.assertEqual(receipt["schema_version"], 2)
        self.assertEqual(receipt["source_kind"], source_kind)
        self.assertEqual(receipt["source_commit"], "a" * 40)
        for field in ("release_sha256", "payload_sha256", "sidecar_sha256"):
            self.assertRegex(receipt[field], r"^[0-9a-f]{64}$")
        self.assertEqual(receipt["asset_sha256"] is None, source_kind == "local_link")

    def test_cli_list_marks_legacy_and_corrupt_links_unavailable(self):
        (self.case.bundle / "release.json").unlink(missing_ok=True)
        self.case.run("pkg", "link", *PIN.values(), self.case.bundle)
        listed = json.loads(self.case.run("pkg", "list", "--json").stdout)
        self.assertEqual(listed[0]["status"], "unverified_legacy_link")
        self.assertFalse(listed[0]["available"])
        self.assertIn("unverified_legacy_link", self.case.run("pkg", "list").stdout)
        self.case.run("pkg", "remove", *PIN.values())
        self.case.link()
        sidecar = self.case.stored() / "node-types.json"
        sidecar.write_bytes(b"{}")
        listed = json.loads(self.case.run("pkg", "list", "--json").stdout)
        self.assertEqual(listed[0]["status"], "unavailable")
        self.assertFalse(listed[0]["available"])
        self.assertNotIn(str(self.case.root), listed[0]["reason"])

    def test_cli_link_and_archive_do_not_activate_deployment(self):
        before = self.case.init()
        self.case.link()
        self.assert_receipt("local_link")
        self.assertEqual(self.case.graph_path.read_bytes(), before)
        listed = json.loads(self.case.run("pkg", "list", "--json").stdout)
        self.assertEqual([(item["id"], item["version"]) for item in listed],
                         [(PIN["id"], PIN["version"])])
        self.assertFalse((self.case.home / "run/manager.sock").exists())

        with tempfile.TemporaryDirectory(prefix="isos-p01-archive-") as temp:
            archived = CleanHome(temp)
            archived.archive.write_bytes(self.case.archive.read_bytes())
            archived.manifest.write_bytes(self.case.manifest.read_bytes())
            original = archived.init()
            archived.run("pkg", "verify", archived.manifest, archived.archive)
            archived.install()
            self.assertEqual(archived.graph_path.read_bytes(), original)
            receipt = archived.receipt()
            self.assertEqual(receipt["schema_version"], 2)
            self.assertEqual(receipt["source_kind"], "local_candidate")
            self.assertEqual(receipt["asset_sha256"], sha256(archived.archive.read_bytes()))
            self.assertIsNone(receipt["local_link"])

    def test_http_verified_link_collision_and_sanitized_diagnostic(self):
        before = self.case.init()
        self.case.start_http()
        body = {**PIN, "path": str(self.case.bundle), "manifest": str(self.case.manifest)}
        status, response = self.case.api("packages/link", "POST", body)
        self.assertEqual(status, 200, response)
        self.assertTrue(response["ok"])
        self.assertTrue(response["data"]["verified"])
        self.assertEqual(response["data"]["source_kind"], "local_link")
        self.assertEqual(response["data"]["installed"][0]["schema_version"], 2)
        status, rejected = self.case.api("packages/link", "POST", body)
        self.assertEqual(status, 409, rejected)
        self.assertFalse(rejected["ok"])
        diagnostic = rejected["data"]["diagnostics"][0]
        self.assertEqual(diagnostic["code"], "package_version_exists")
        self.assertEqual(diagnostic["id"], PIN["id"])
        self.assertEqual(diagnostic["version"], PIN["version"])
        self.assertEqual(diagnostic["path"], "")
        self.assertNotIn(str(self.case.root), json.dumps(rejected))
        self.assertEqual(self.case.graph_path.read_bytes(), before)

    def test_link_tamper_blocks_apply_and_run_without_revision_change(self):
        self.case.init()
        self.case.link()
        applied_bytes = self.case.apply_bundle_graph()
        self.case.start_http()
        _, before_status = self.case.api("process-status")
        before_active = before_status["data"].get("active_revision")
        before_processes = before_status["data"]["processes"]
        before_state = before_status["data"]["state"]
        saved_revision = sha256(applied_bytes)
        edit = {"schema_version": 1, "revision": saved_revision,
                "graph": json.loads(applied_bytes)}
        mutations = [
            (self.case.bundle / "bin/eos-mujoco-sim", "package_changed", "payload"),
            (self.case.stored() / "release.json", "package_changed", "manifest"),
            (self.case.stored() / "node-types.json", "package_invalid_sidecar", "sidecar"),
        ]
        for path, code, stage in mutations:
            with self.subTest(path=path.name, stage=stage):
                original = path.read_bytes()
                path.write_bytes(original + b"\n")
                try:
                    status, rejected = self.case.api("node-graph", "PUT", edit)
                    self.assertEqual(status, 422, rejected)
                    self.assertFalse(rejected["ok"])
                    diagnostic = rejected["data"]["diagnostics"][0]
                    self.assertEqual(diagnostic["code"], code)
                    self.assertEqual(diagnostic["stage"], stage)
                    self.assertEqual(diagnostic["id"], PIN["id"])
                    self.assertEqual(diagnostic["version"], PIN["version"])
                    self.assertTrue(diagnostic["path"].startswith("/cores/eos/"))
                    self.assertNotIn(str(self.case.root), json.dumps(rejected))
                    status, rejected = self.case.api("process-control", "POST", {
                        "schema_version": 1, "operation": "run", "revision": saved_revision})
                    self.assertEqual(status, 422, rejected)
                    self.assertFalse(rejected["ok"])
                    self.assertNotIn(str(self.case.root), json.dumps(rejected))
                    self.assertEqual(self.case.graph_path.read_bytes(), applied_bytes)
                    _, after_status = self.case.api("process-status")
                    self.assertEqual(after_status["data"].get("active_revision"), before_active)
                    self.assertEqual(after_status["data"]["processes"], before_processes)
                    self.assertEqual(after_status["data"]["state"], before_state)
                finally:
                    path.write_bytes(original)

    def test_new_graph_pin_revalidation_on_link_change(self):
        self.case.init()
        self.case.link()
        applied_bytes = self.case.apply_bundle_graph(legacy=False)
        self.case.start_http()
        _, before = self.case.api("process-status")
        executable = self.case.bundle / "bin/eos-mujoco-sim"
        executable.write_bytes(executable.read_bytes() + b"# changed after verification\n")
        status, rejected = self.case.api("node-graph", "PUT", {
            "schema_version": 1, "revision": sha256(applied_bytes),
            "graph": json.loads(applied_bytes)})
        self.assertEqual(status, 422, rejected)
        diagnostic = rejected["data"]["diagnostics"][0]
        self.assertEqual(diagnostic["code"], "package_changed")
        self.assertEqual(diagnostic["stage"], "payload")
        self.assertEqual(diagnostic["id"], PIN["id"])
        self.assertEqual(diagnostic["version"], PIN["version"])
        self.assertTrue(diagnostic["path"].startswith("/cores/eos/"))
        self.assertNotIn(str(self.case.root), json.dumps(rejected))
        self.assertEqual(self.case.graph_path.read_bytes(), applied_bytes)
        _, after = self.case.api("process-status")
        self.assertEqual(after["data"]["active_revision"], before["data"]["active_revision"])
        self.assertEqual(after["data"]["processes"], before["data"]["processes"])

    def test_archive_digest_rejection_publishes_nothing(self):
        before = self.case.init()
        self.case.archive.write_bytes(self.case.archive.read_bytes() + b"tampered archive")
        rejected = self.case.run("pkg", "install", "--manifest", self.case.manifest,
                                 "--asset", self.case.archive, "--allow-unapproved",
                                 success=False)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertFalse(self.case.stored().exists())
        self.assertEqual(self.case.graph_path.read_bytes(), before)

    def test_http_exact_pin_mismatch_is_structured_and_private(self):
        before = self.case.init()
        self.case.start_http()
        wrong = {**PIN, "version": "0.1.2", "path": str(self.case.bundle),
                 "manifest": str(self.case.manifest)}
        status, rejected = self.case.api("packages/link", "POST", wrong)
        self.assertEqual(status, 422, rejected)
        self.assertFalse(rejected["ok"])
        diagnostic = rejected["data"]["diagnostics"][0]
        self.assertEqual(diagnostic["code"], "package_invalid_manifest")
        self.assertEqual(diagnostic["stage"], "manifest")
        self.assertEqual(diagnostic["id"], PIN["id"])
        self.assertEqual(diagnostic["version"], "0.1.2")
        self.assertEqual(diagnostic["path"], "")
        self.assertNotIn(str(self.case.root), json.dumps(rejected))
        self.assertFalse(self.case.stored().exists())
        self.assertEqual(self.case.graph_path.read_bytes(), before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
