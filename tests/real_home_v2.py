#!/usr/bin/env python3
"""Isolated IsOS → EOS → real MuJoCo smoke test; requires a staged v2 bundle."""

import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import signal
import socket
import statistics
import subprocess
import tempfile
import time
from urllib.request import Request, urlopen


def port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def command(isos, env, *args, success=True):
    result = subprocess.run([str(isos), *args], env=env, text=True,
                            capture_output=True, timeout=30)
    print("$ isos", *args, "exit", result.returncode, flush=True)
    print(result.stdout.strip(), result.stderr.strip(), flush=True)
    if success and result.returncode:
        raise RuntimeError("IsOS command failed")
    return result


def api(isos_port, path, method="GET", data=None):
    request = Request(f"http://127.0.0.1:{isos_port}/api/isos/v2/{path}",
                      data=None if data is None else json.dumps(data).encode(),
                      method=method, headers={"Content-Type": "application/json",
                                              "Origin": f"http://127.0.0.1:{isos_port}"})
    with urlopen(request, timeout=5) as response:
        body = json.load(response)
    if not body["ok"]:
        raise RuntimeError(body)
    return body["data"]


def eos(port_number, payload):
    request = Request(f"http://127.0.0.1:{port_number}/api/eos/v2/control",
                      data=json.dumps(payload).encode(),
                      headers={"Content-Type": "application/json"})
    with urlopen(request, timeout=5) as response:
        body = json.load(response)
    if not body["ok"]:
        raise RuntimeError(body)
    return body


def await_condition(check, timeout=20):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            result = check()
            if result:
                return result
        except Exception as error:
            last = error
        time.sleep(0.1)
    raise RuntimeError(f"condition did not become ready: {last}")


def main():
    isos = Path(os.environ["ISOS_BIN"])
    bundle = Path(os.environ["ISOS_TEST_REAL_BUNDLE"])
    aether = Path(os.environ["ISOS_AETHER_BIN_DIR"])
    mujoco = Path(os.environ["MUJOCO_ROOT"])
    ui_repo = Path(os.environ.get("ISOSUI_REPO_PATH", str(Path(__file__).resolve().parents[2] / "IsosUI")))
    if not (ui_repo / "node_modules").is_dir():
        raise RuntimeError(f"install IsosUI dependencies first: {ui_repo}")
    descriptor = json.loads((bundle / "backend-v2.json").read_text())
    assert descriptor["entrypoint"] == "bin/eos-mujoco-sim"
    scene = descriptor["scenes"][0]["id"]
    print(f"Environment: IsOS={isos} Aether={aether} bundle={bundle} MuJoCo={mujoco} ui_repo={ui_repo} DISPLAY={os.environ.get('DISPLAY')} WAYLAND_DISPLAY={os.environ.get('WAYLAND_DISPLAY')}", flush=True)
    with tempfile.TemporaryDirectory(prefix="isos-real-v2-") as temporary:
        root = Path(temporary)
        linked_bundle = root / "bundle"
        shutil.copytree(bundle, linked_bundle)
        isos_port, ui_port, bridge_port, config_port = port(), port(), port(), port()
        env = dict(os.environ, ISOS_HOME=str(root / "home"),
                   ISOS_HTTP_PORT=str(isos_port),
                   ISOSUI_PORT=str(ui_port),
                   ISOS_AETHER_BIN_DIR=str(aether), MUJOCO_ROOT=str(mujoco),
                   ISOSUI_REPO_PATH=str(ui_repo))
        command(isos, env, "pkg", "link", "island/mujoco-sim", "2.0.0", str(linked_bundle))
        init_log = (root / "init.log").open("w+")
        init = subprocess.Popen([str(isos), "init"], env=env, stdout=init_log,
                                stderr=subprocess.STDOUT, text=True)
        running = None
        try:
            await_condition(lambda: (root / "home/run/manager.sock").exists())
            graph_file = root / "home/config/deployments/default.nodes.json"
            graph = json.loads(graph_file.read_text())
            backend = graph["cores"]["isos"]["layers"]["process"]["groups"]["backend"]["nodes"]["simulator"]["settings"]
            backend["scene"] = scene
            backend["env"] = {"MUJOCO_GUI": "1"}
            components = graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]
            for node, listen in [("bridge", bridge_port), ("config", config_port)]:
                settings = components[node]["settings"]
                settings["command"][-1] = f"127.0.0.1:{listen}"
                settings["ready_tcp"] = f"127.0.0.1:{listen}"
            document = await_condition(lambda: api(isos_port, "node-graph"))
            assert api(isos_port, "node-graph/validate", "POST", {"schema_version": 2, "graph": graph})["valid"]
            applied = api(isos_port, "node-graph", "PUT", {"schema_version": 2,
                                                           "revision": document["revision"], "graph": graph})
            revision = applied["revision"]
            print("Graph validated and applied:", revision, flush=True)
            command(isos, env, "config", "validate", str(graph_file))
            startup_begin = time.perf_counter()
            running = subprocess.Popen([str(isos), "run"], env=env,
                                       stdout=(root / "run.log").open("w+"),
                                       stderr=subprocess.STDOUT, text=True)
            status = await_condition(lambda: api(isos_port, "process-status") if
                                     api(isos_port, "process-status")["state"] == "running" else None)
            startup_ms = (time.perf_counter() - startup_begin) * 1000
            assert {p["id"] for p in status["processes"]} == {"simulator", "eos", "bridge", "config"}
            assert status["active_revision"] == revision
            assert hashlib.sha256(graph_file.read_bytes()).hexdigest() == revision
            if env.get("DISPLAY") and shutil.which("xwininfo"):
                def gui_visible():
                    window = subprocess.run(["xwininfo", "-name", "IsOS MuJoCo — live simulation"],
                                            env=env, text=True, capture_output=True, timeout=3)
                    return "Map State: IsViewable" in window.stdout
                assert await_condition(gui_visible), "MuJoCo GUI window is not viewable"
                print("MuJoCo GUI window mapped IsViewable: PASS", flush=True)
            else:
                print("MuJoCo GUI visibility check skipped: DISPLAY or xwininfo unavailable", flush=True)
            assert command(isos, env, "pkg", "remove", "island/mujoco-sim", "2.0.0", success=False).returncode != 0
            assert eos(bridge_port, {"op": "discover"})["nodes"]
            camera = "cores/eos/layers/hardware/groups/camera/nodes/front"
            base = "cores/eos/layers/hardware/groups/base/nodes/drive"
            arm = "cores/eos/layers/hardware/groups/manipulator/nodes/arm"
            assert eos(bridge_port, {"op": "capture", "node_path": camera})["frame"]["width_px"] > 0
            samples = []
            for _ in range(20):
                started = time.perf_counter()
                eos(bridge_port, {"op": "capture", "node_path": camera})
                samples.append((time.perf_counter() - started) * 1000)
            p99 = sorted(samples)[math.ceil(0.99 * len(samples)) - 1]
            print(f"BENCH real startup_ms={startup_ms:.3f} capture_n=20 median_ms={statistics.median(samples):.3f} p99_ms={p99:.3f} throughput_rps={20 / (sum(samples) / 1000):.1f}", flush=True)
            assert eos(bridge_port, {"op": "observe", "node_path": base})["observation"]["valid"]
            assert len(eos(bridge_port, {"op": "observe", "node_path": arm})["observation"]["joint_positions_rad"]) == 6
            now = eos(bridge_port, {"op": "time"})["monotonic_ns"]
            base_result = eos(bridge_port, {"op": "dispatch", "node_path": base,
                                            "command_id": "real-base-1",
                                            "deadline_monotonic_ns": now + 5_000_000_000,
                                            "movement": {"type": "body_velocity", "x_m_s": 0.05,
                                                         "y_m_s": 0.0, "yaw_rad_s": 0.0,
                                                         "valid_for_ms": 100}})
            assert base_result["result"]["status"] != "Rejected"
            arm_result = eos(bridge_port, {"op": "dispatch", "node_path": arm,
                                           "command_id": "real-arm-1",
                                           "deadline_monotonic_ns": now + 5_000_000_000,
                                           "movement": {"type": "joint_positions",
                                                        "joint_names": [f"joint{i}" for i in range(1, 7)],
                                                        "joint_positions_rad": [0.0] * 6}})
            assert arm_result["result"]["status"] != "Rejected"
            print("Real base and manipulator dispatch:", base_result["result"]["status"],
                  arm_result["result"]["status"], flush=True)
            assert eos(bridge_port, {"op": "stop", "node_path": base})["ok"]
            assert eos(bridge_port, {"op": "stop", "node_path": arm})["ok"]
            nested_asset = linked_bundle / "assets/menagerie/robotiq_2f85/2f85.xml"
            original_asset = nested_asset.read_bytes()
            nested_asset.write_bytes(original_asset + b"\n")
            assert command(isos, env, "start", success=False).returncode != 0
            nested_asset.write_bytes(original_asset)
            print("Changed nested linked asset blocked active start: PASS", flush=True)
            restarted = api(isos_port, "process-control", "POST",
                            {"schema_version": 2, "operation": "restart", "revision": revision})
            assert restarted["started"][0] == "simulator"
            assert api(isos_port, "process-logs/simulator?lines=10")["node"] == "simulator"
            print("Real MuJoCo discovery, capture, observations, stops, restart, logs: PASS", flush=True)
            running.send_signal(signal.SIGINT)
            assert running.wait(timeout=10) == 0
            assert await_condition(lambda: api(isos_port, "process-status")["state"] == "stopped")
            print("Ctrl+C stopped real MuJoCo and EOS: PASS", flush=True)
        finally:
            if running is not None and running.poll() is None:
                running.send_signal(signal.SIGINT)
                running.wait(timeout=10)
            if init.poll() is None:
                init.send_signal(signal.SIGINT)
            init.wait(timeout=10)
            init_log.close()
            print("--- init log ---\n", (root / "init.log").read_text(), flush=True)
            if (root / "run.log").exists():
                print("--- run log ---\n", (root / "run.log").read_text(), flush=True)
            for name in ["simulator", "eos", "bridge", "config"]:
                log = root / "home/data/process-logs/default" / f"{name}.log"
                if log.exists():
                    print(f"--- {name} log ---\n", log.read_text(errors="replace")[-6000:], flush=True)


if __name__ == "__main__":
    main()
