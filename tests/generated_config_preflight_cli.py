#!/usr/bin/env python3
"""P04 clean-home generated configuration and launch-preflight replay.

Build IsOS and EOS first, then set ISOS_BIN and ISOS_AETHER_BIN_DIR if their
debug binaries are outside the sibling checkouts.
"""

import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import unittest
from urllib.error import HTTPError
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
ISOS = Path(os.environ.get("ISOS_BIN", ROOT / "target/debug/isos"))
EOS_BIN_DIR = Path(os.environ.get(
    "ISOS_AETHER_BIN_DIR", ROOT.parent / "Island-eos/target/debug"
))
CASES = json.loads((ROOT / "tests/fixtures/generated_config_preflight/cases.json").read_text())


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def digest(data):
    return hashlib.sha256(data).hexdigest()


class CleanHome:
    def __init__(self, root):
        self.root = Path(root)
        self.home = self.root / "home"
        self.graph_path = self.home / "config/deployments/default.nodes.json"
        self.generated = self.home / "data/generated/default"
        self.port = free_port()
        self.ui_port = free_port()
        self.server = None
        self.output = None
        self.env = dict(os.environ, ISOS_HOME=str(self.home),
                        ISOS_AETHER_BIN_DIR=str(EOS_BIN_DIR),
                        ISOS_HTTP_PORT=str(self.port), ISOSUI_PORT=str(self.ui_port))
        self.env.pop("P04_MISSING_SECRET", None)

    def command(self, *args, success=True):
        result = subprocess.run([str(ISOS), *map(str, args)], env=self.env,
                                text=True, capture_output=True, timeout=45)
        print("$ isos", *args, "=>", result.returncode, flush=True)
        if result.stdout:
            print(result.stdout.rstrip(), flush=True)
        if result.stderr:
            print(result.stderr.rstrip(), flush=True)
        if success and result.returncode:
            raise AssertionError(f"isos {' '.join(map(str, args))} failed")
        return result

    def init(self):
        self.command("init", "--mode", "dev")
        graph = json.loads(self.graph_path.read_text())
        assert graph["version"] == "0.1.0", graph["version"]
        return graph

    def apply(self, graph=None):
        if graph is None:
            graph = json.loads(self.graph_path.read_text())
        candidate = self.root / "candidate.nodes.json"
        candidate.write_text(json.dumps(graph, indent=2))
        saved_revision = digest(self.graph_path.read_bytes())
        self.command("config", "validate", candidate)
        self.command("config", "apply", candidate, "--revision", saved_revision)
        return digest(self.graph_path.read_bytes())

    def selected(self):
        revision = digest(self.graph_path.read_bytes())
        selection = json.loads((self.generated / "selection.json").read_text())
        assert selection["schema_version"] == 2
        entry = selection["entries"][revision]
        assert entry["kind"] == "v2"
        generation = entry["generation"]
        leaf = self.generated / "v2" / revision / generation
        return revision, generation, leaf

    def start_server(self):
        self.output = (self.root / "serve.log").open("w+")
        self.server = subprocess.Popen([str(ISOS), "serve"], env=self.env,
                                       stdout=self.output, stderr=subprocess.STDOUT, text=True)
        for _ in range(160):
            if self.server.poll() is not None:
                raise AssertionError(f"IsOS serve exited {self.server.returncode}")
            try:
                self.api("node-graph")
                return
            except (OSError, ValueError):
                time.sleep(0.05)
        raise AssertionError("IsOS API did not become ready")

    def api(self, route, data=None):
        payload = None if data is None else json.dumps(data).encode()
        request = Request(f"http://127.0.0.1:{self.port}/api/isos/v1/{route}",
                          data=payload, method="POST" if data is not None else "GET",
                          headers={"Origin": f"http://127.0.0.1:{self.port}",
                                   "Content-Type": "application/json"})
        try:
            with urlopen(request, timeout=30) as response:
                return response.status, json.load(response)
        except HTTPError as error:
            return error.code, json.load(error)

    def control(self, operation):
        return self.api("process-control", {
            "schema_version": 1, "operation": operation,
            "revision": digest(self.graph_path.read_bytes()),
        })

    def close(self):
        if self.server is not None:
            self.server.terminate()
            try:
                self.server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.server.kill()
                self.server.wait(timeout=5)
        if self.output is not None:
            self.output.close()


class GeneratedConfigReplay(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for binary in (ISOS, EOS_BIN_DIR / "aether-profile", EOS_BIN_DIR / "aether-runtime"):
            if not binary.is_file():
                raise AssertionError(f"missing test binary: {binary}")
        assert CASES["slice"] == "P04"

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="isos-p04-")
        self.case = CleanHome(self.temp.name)
        self.case.init()

    def tearDown(self):
        self.case.close()
        self.temp.cleanup()

    def assert_failure(self, expected_case):
        expected = next(case for case in CASES["cases"] if case["id"] == expected_case)
        status, reply = self.case.control("run")
        self.assertEqual(status, 422, reply)
        self.assertFalse(reply["ok"])
        self.assertIsInstance(reply.get("data"), dict, reply)
        self.assertIn("diagnostics", reply["data"])
        first = reply["data"]["diagnostics"][0]
        self.assertEqual(
            {key: first[key] for key in ("code", "path", "stage")},
            {"code": expected["expected_code"], "path": expected["expected_path"],
             "stage": expected["expected_stage"]},
        )
        self.assertEqual(set(first), {"code", "path", "message", "stage"})

    def test_immutable_selection_and_repeat_apply(self):
        graph_before = self.case.graph_path.read_bytes()
        self.case.apply()
        revision, generation, leaf = self.case.selected()
        self.assertEqual(revision, digest(self.case.graph_path.read_bytes()))
        marker = self.case.generated / "v2/required" / f"{revision}.json"
        self.assertEqual(json.loads(marker.read_text()),
                         {"schema_version": 1, "revision": revision})
        manifest = json.loads((leaf / "manifest.json").read_text())
        self.assertEqual(manifest["schema_version"], 2)
        self.assertEqual((manifest["revision"], manifest["generation"]),
                         (revision, generation))
        inventory = {key: value for key, value in manifest.items() if key != "generation"}
        canonical = json.dumps(inventory, sort_keys=True, separators=(",", ":"),
                               ensure_ascii=False).encode()
        self.assertEqual(digest(canonical), generation)
        self.assertEqual([item["path"] for item in manifest["files"]],
                         sorted(item["path"] for item in manifest["files"]))
        listed = {item["path"] for item in manifest["files"]}
        present = {str(path.relative_to(leaf)) for path in leaf.rglob("*") if path.is_file()}
        self.assertEqual(present, listed | {"manifest.json"})
        for item in manifest["files"]:
            path = leaf / item["path"]
            self.assertEqual(digest(path.read_bytes()), item["sha256"])
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(leaf.stat().st_mode & 0o777, 0o700)
        self.assertEqual(marker.stat().st_mode & 0o777, 0o600)
        self.assertEqual((leaf / "manifest.json").stat().st_mode & 0o777, 0o600)
        manifest_before = (leaf / "manifest.json").read_bytes()
        self.case.apply()
        self.assertEqual(self.case.selected()[:2], (revision, generation))
        self.assertEqual((leaf / "manifest.json").read_bytes(), manifest_before)
        self.assertEqual(self.case.graph_path.read_bytes(), graph_before)

    def test_deleted_selection_cannot_downgrade_to_g03(self):
        self.case.apply()
        before = self.case.graph_path.read_bytes()
        (self.case.generated / "selection.json").unlink()
        self.case.start_server()
        self.assert_failure("deleted_selection_after_v2_apply")
        self.assertEqual(self.case.graph_path.read_bytes(), before)

    def test_generated_file_tamper_rejected_before_run(self):
        self.case.apply()
        before = self.case.graph_path.read_bytes()
        _, _, leaf = self.case.selected()
        with (leaf / "eos/graph.json").open("ab") as file:
            file.write(b"\n")
        self.case.start_server()
        self.assert_failure("manifest_file_tamper")
        self.assertEqual(self.case.graph_path.read_bytes(), before)

    def test_restart_tamper_keeps_active_children(self):
        self.case.apply()
        self.case.start_server()
        status, reply = self.case.control("run")
        self.assertEqual(status, 200, reply)
        status, reply = self.case.api("process-status")
        self.assertEqual(status, 200, reply)
        active = reply["data"]
        self.assertEqual(active["state"], "running")
        pids = {process["id"]: process["pid"] for process in active["processes"]}
        self.assertTrue(pids)
        _, _, leaf = self.case.selected()
        with (leaf / "eos/graph.json").open("ab") as file:
            file.write(b"\n")
        status, rejected = self.case.control("restart")
        self.assertEqual(status, 422, rejected)
        first = rejected["data"]["diagnostics"][0]
        self.assertEqual((first["code"], first["path"], first["stage"]),
                         ("generated_manifest_mismatch", "", "snapshot"))
        status, reply = self.case.api("process-status")
        self.assertEqual(status, 200, reply)
        after = reply["data"]
        self.assertEqual(after["active_revision"], active["active_revision"])
        self.assertEqual({process["id"]: process["pid"] for process in after["processes"]}, pids)

    def test_missing_secret_has_exact_pointer_and_no_value_leak(self):
        graph = json.loads(self.case.graph_path.read_text())
        eos = graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["eos"]
        eos["settings"]["env_from"] = {"P04_TARGET": "P04_MISSING_SECRET"}
        self.case.apply(graph)
        _, _, leaf = self.case.selected()
        self.assertNotIn(b"secret-value", (leaf / "manifest.json").read_bytes())
        self.case.start_server()
        status, reply = self.case.control("run")
        self.assertEqual(status, 422, reply)
        first = reply["data"]["diagnostics"][0]
        self.assertEqual(
            (first["code"], first["path"], first["stage"]),
            ("generated_missing_secret",
             "/cores/isos/layers/process/groups/component/nodes/eos/settings/env_from/P04_TARGET",
             "spawn"),
        )
        self.assertNotIn("P04_MISSING_SECRET", json.dumps(reply))


if __name__ == "__main__":
    unittest.main(verbosity=2)
