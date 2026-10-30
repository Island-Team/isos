#!/usr/bin/env python3
"""Fixed IsOS→EOS→mock MuJoCo scenario and startup/socket benchmark.

Requires built IsOS and Aether Rust binaries. Uses a protocol stub, not MuJoCo physics.
"""
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

ROOT = Path(__file__).resolve().parents[1]
EOS_BIN = Path(os.environ.get("AETHER_BIN_DIR", str(ROOT.parent / "Island-eos/target/debug")))
ISOS = Path(os.environ.get("ISOS_BIN", str(ROOT / "target/debug/isos")))
MOCK = ROOT / "tests/mock_mujoco_v2.py"


def run(*args, env, expect=True):
    command = [str(ISOS), *args]
    result = subprocess.run(command, env=env, text=True, capture_output=True, timeout=30)
    print("$", " ".join(command), flush=True)
    if result.stdout:
        print(result.stdout.rstrip(), flush=True)
    if result.stderr:
        print(result.stderr.rstrip(), flush=True)
    if expect and result.returncode:
        raise RuntimeError(f"exit {result.returncode}")
    return result


def control(value, port):
    request = Request(f"http://127.0.0.1:{port}/api/eos/v2/control",
                      data=json.dumps(value).encode(), headers={"Content-Type": "application/json"})
    with urlopen(request, timeout=3) as response:
        body = json.load(response)
    if not body.get("ok"):
        raise RuntimeError(f"EOS control {value['op']}: {body}")
    return body


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def main():
    for binary in [ISOS, EOS_BIN / "aether-profile", EOS_BIN / "aether-runtime",
                   EOS_BIN / "aether-http-bridge", EOS_BIN / "aether-config-server"]:
        if not binary.is_file():
            raise RuntimeError(f"build first: {binary}")
    version = subprocess.run([str(ISOS), "--version"], text=True, capture_output=True, check=True)
    print(version.stdout.strip(), flush=True)
    with tempfile.TemporaryDirectory(prefix="isos-v2-clean-") as temp:
        root = Path(temp)
        home = root / "home"
        bundle = root / "bundle"
        (bundle / "bin").mkdir(parents=True)
        (bundle / "assets/scenes/navigation").mkdir(parents=True)
        shutil.copy2(MOCK, bundle / "bin/eos-mujoco-sim")
        (bundle / "assets/scene.xml").write_text("<mujoco/>")
        (bundle / "assets/scenes/navigation/scene.xml").write_text("<mujoco/>")
        descriptor = {
            "schema_version": "2.0.0", "package": {"id": "island/mujoco-sim", "version": "2.0.0"},
            "entrypoint": "bin/eos-mujoco-sim", "protocol": "mujoco-v2",
            "scenes": [{"id": "default", "path": "assets/scene.xml"},
                       {"id": "navigation", "path": "assets/scenes/navigation/scene.xml"}],
            "devices": {"camera": "cam-0", "base": "base-0", "manipulator": "arm-0"},
            "runtime": {"mujoco_version": "3.16.0", "environment_variable": "MUJOCO_ROOT",
                        "library": "lib/libmujoco.so.3.16.0"},
        }
        (bundle / "backend-v2.json").write_text(json.dumps(descriptor))
        library = root / "mujoco/lib"
        library.mkdir(parents=True)
        (library / "libmujoco.so.3.16.0").write_bytes(b"test-only dependency stub")
        web = Path(os.environ["ISOS_TEST_WEB_DIST"]) if "ISOS_TEST_WEB_DIST" in os.environ else root / "web"
        if "ISOS_TEST_WEB_DIST" not in os.environ:
            web.mkdir()
            (web / "index.html").write_text("<html><body>IsOS test workbench</body></html>")
        isos_port = free_port()
        env = dict(os.environ, ISOS_HOME=str(home), MUJOCO_ROOT=str(library.parent),
                   ISOS_HTTP_PORT=str(isos_port), ISOS_WEB_DIST=str(web),
                   ISOS_AETHER_PROFILE_BIN=str(EOS_BIN / "aether-profile"),
                   ISOS_EOSD_BIN=str(EOS_BIN / "aether-runtime"),
                   ISOS_EOS_NORTH_HTTP_BIN=str(EOS_BIN / "aether-http-bridge"),
                   ISOS_AETHER_CONFIG_SERVER_BIN=str(EOS_BIN / "aether-config-server"))
        run("pkg", "link", "island/mujoco-sim", "2.0.0", str(bundle), env=env)
        init_log = (root / "init.log").open("w+")
        init_process = subprocess.Popen([str(ISOS), "init"], env=env,
                                        stdout=init_log, stderr=subprocess.STDOUT, text=True)
        run_process = None
        shutdown_pids = []
        for _ in range(100):
            if (home / "run/manager.sock").exists():
                break
            if init_process.poll() is not None:
                raise RuntimeError(f"isos init exited {init_process.returncode}")
            time.sleep(0.05)
        else:
            raise RuntimeError("isos init did not bind its control socket")
        graph_path = home / "config/deployments/default.nodes.json"
        graph = json.loads(graph_path.read_text())
        bridge_port, config_port = free_port(), free_port()
        bridge = graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["bridge"]["settings"]
        bridge["command"][-1] = f"127.0.0.1:{bridge_port}"
        bridge["ready_tcp"] = f"127.0.0.1:{bridge_port}"
        config = graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["config"]["settings"]
        config["command"][-1] = f"127.0.0.1:{config_port}"
        config["ready_tcp"] = f"127.0.0.1:{config_port}"
        graph_path.write_text(json.dumps(graph, indent=2))
        run("config", "validate", str(graph_path), env=env)
        missing = bundle / "assets/scene.xml"
        missing.write_text('<mujoco><mesh file="missing.stl"/></mujoco>')
        assert run("config", "validate", str(graph_path), env=env, expect=False).returncode != 0
        missing.write_text("<mujoco/>")
        bad_graph = json.loads(graph_path.read_text())
        bad_graph["cores"]["eos"]["layers"]["hardware"]["groups"]["base"]["nodes"]["drive"]["adapter"]["settings"]["device_id"] = "wrong-base"
        bad_path = root / "bad.nodes.json"
        bad_path.write_text(json.dumps(bad_graph))
        assert run("config", "validate", str(bad_path), env=env, expect=False).returncode != 0
        run("config", "apply", str(graph_path), env=env)
        assert run("pkg", "remove", "island/mujoco-sim", "2.0.0", env=env, expect=False).returncode != 0
        saved_hash = hashlib.sha256(graph_path.read_bytes()).hexdigest()
        try:
            def isos_api(path, method="GET", data=None):
                payload = None if data is None else json.dumps(data).encode()
                request = Request(f"http://127.0.0.1:{isos_port}/api/isos/v2/{path}",
                                  data=payload, method=method,
                                  headers={"Origin": f"http://127.0.0.1:{isos_port}",
                                           "Content-Type": "application/json"})
                with urlopen(request, timeout=4) as response:
                    envelope = json.load(response)
                if not envelope["ok"]:
                    raise RuntimeError(envelope)
                return envelope["data"]
            for _ in range(100):
                try:
                    document = isos_api("node-graph")
                    break
                except Exception:
                    time.sleep(0.05)
            else:
                raise RuntimeError("IsOS HTTP API did not appear")
            assert document["revision"] == saved_hash
            with urlopen(f"http://127.0.0.1:{isos_port}/en/isos", timeout=4) as response:
                assert b"<html" in response.read().lower()
            print("ISOS WORKBENCH static page: PASS")
            assert isos_api("backends")["backends"][0]["scenes"] == ["default", "navigation"]
            assert isos_api("setup-status")["ready"]
            assert isos_api("node-graph/validate", "POST", {"schema_version": 2, "graph": document["graph"]})["valid"]
            startups = []
            run_log = (root / "run.log").open("w+")
            start = time.perf_counter()
            run_process = subprocess.Popen([str(ISOS), "run"], env=env,
                                           stdout=run_log, stderr=subprocess.STDOUT, text=True)
            for _ in range(200):
                if isos_api("process-status")["state"] == "running":
                    break
                if run_process.poll() is not None:
                    raise RuntimeError(f"isos run exited {run_process.returncode}")
                time.sleep(0.05)
            else:
                raise RuntimeError("isos run did not start")
            startups.append((time.perf_counter() - start) * 1000)
            for _ in range(4):
                start = time.perf_counter()
                result = isos_api("process-control", "POST", {"schema_version": 2, "operation": "restart"})
                assert result["started"][0] == "simulator"
                startups.append((time.perf_counter() - start) * 1000)
            status = json.loads(run("status", "--json", env=env).stdout)
            assert status["state"] == "running" and status["package"]["version"] == "2.0.0"
            assert {p["id"] for p in status["processes"]} == {"simulator", "eos", "bridge", "config"}
            assert hashlib.sha256(graph_path.read_bytes()).hexdigest() == saved_hash, "EOS changed applied graph"
            discovered = control({"op": "discover"}, bridge_port)
            assert len(discovered["nodes"]) >= 3
            camera = "cores/eos/layers/hardware/groups/camera/nodes/front"
            base = "cores/eos/layers/hardware/groups/base/nodes/drive"
            arm = "cores/eos/layers/hardware/groups/manipulator/nodes/arm"
            frame = control({"op": "capture", "node_path": camera}, bridge_port)
            assert frame["frame"]["width_px"] == 1
            assert control({"op": "observe", "node_path": base}, bridge_port)["observation"]["valid"]
            assert len(control({"op": "observe", "node_path": arm}, bridge_port)["observation"]["joint_positions_rad"]) == 6
            now = control({"op": "time"}, bridge_port)["monotonic_ns"]
            base_cmd = {"op": "dispatch", "node_path": base, "command_id": "mock-base-1",
                        "deadline_monotonic_ns": now + 5_000_000_000,
                        "movement": {"type": "body_velocity", "x_m_s": 0.1, "y_m_s": 0.0,
                                     "yaw_rad_s": 0.0, "valid_for_ms": 100}}
            assert control(base_cmd, bridge_port)["result"]["status"] != "Rejected"
            arm_cmd = {"op": "dispatch", "node_path": arm, "command_id": "mock-arm-1",
                       "deadline_monotonic_ns": now + 5_000_000_000,
                       "movement": {"type": "joint_positions", "joint_names": [f"joint{i}" for i in range(1, 7)],
                                    "joint_positions_rad": [0.0] * 6}}
            assert control(arm_cmd, bridge_port)["result"]["status"] != "Rejected"
            assert control({"op": "stop", "node_path": base}, bridge_port)["ok"]
            assert control({"op": "stop", "node_path": arm}, bridge_port)["ok"]
            print("OPERATIONS discovery, camera capture, base/manipulator observation and command, safe stop: PASS")
            samples = []
            for _ in range(200):
                start = time.perf_counter()
                control({"op": "time"}, bridge_port)
                samples.append((time.perf_counter() - start) * 1000)
            run("logs", "simulator", "--lines", "5", env=env)
            run_process.send_signal(signal.SIGINT)
            assert run_process.wait(timeout=10) == 0
            print("ISOS RUN Ctrl+C stopped managed children: PASS")
            assert json.loads(run("status", "--json", env=env).stdout)["state"] == "stopped"
            assert isos_api("process-status")["state"] == "stopped"
            api_run = isos_api("process-control", "POST", {"schema_version": 2, "operation": "run", "revision": saved_hash})
            assert api_run["started"][0] == "simulator"
            assert isos_api("process-logs/simulator?lines=5")["node"] == "simulator"
            assert isos_api("process-control", "POST", {"schema_version": 2, "operation": "stop"})["stopped"][-1] == "simulator"
            print("ISOS HTTP Home, graph validation, process control, status, and logs: PASS")
            missing.write_text('<mujoco><mesh file="gone.stl"/></mujoco>')
            assert run("run", env=env, expect=False).returncode != 0
            failed_status = json.loads(run("status", "--json", env=env).stdout)
            assert "gone.stl" in failed_status["last_error"]
            missing.write_text("<mujoco/>")
            print("FAILURE missing linked asset blocked run and recorded last_error: PASS")
            assert isos_api("process-control", "POST", {"schema_version": 2, "operation": "run"})["started"][0] == "simulator"
            shutdown_pids = [process["pid"] for process in isos_api("process-status")["processes"]]
            print("BENCH fixed workload: 5 process starts/restarts; 200 HTTP→EOS socket time requests")
            print(f"BENCH startup median_ms={statistics.median(startups):.3f} p99_ms={percentile(startups,.99):.3f}")
            print(f"BENCH socket median_ms={statistics.median(samples):.3f} p99_ms={percentile(samples,.99):.3f} throughput_rps={200 / (sum(samples)/1000):.1f}")
            print("PASS clean-home IsOS→EOS→mock MuJoCo scenario")
        finally:
            if run_process is not None and run_process.poll() is None:
                run_process.send_signal(signal.SIGINT)
                run_process.wait(timeout=10)
            if init_process.poll() is None:
                init_process.send_signal(signal.SIGINT)
            assert init_process.wait(timeout=10) == 0
            for pid in shutdown_pids:
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    continue
                raise AssertionError(f"managed child {pid} survived init Ctrl+C")
            assert not (home / "run/manager.sock").exists()
            init_log.close()
            print("ISOS INIT Ctrl+C stopped foreground server: PASS")


if __name__ == "__main__":
    main()
