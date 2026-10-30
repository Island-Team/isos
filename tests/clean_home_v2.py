#!/usr/bin/env python3
"""Fixed IsOS→EOS→mock MuJoCo scenario and startup/socket benchmark.

Requires built IsOS and Aether Rust binaries. Uses a protocol stub, not MuJoCo physics.
"""
import hashlib
import importlib.util
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
from urllib.error import HTTPError

ROOT = Path(__file__).resolve().parents[1]
EOS_BIN = Path(os.environ.get("AETHER_BIN_DIR", str(ROOT.parent / "Island-eos/target/debug")))
ISOS = Path(os.environ.get("ISOS_BIN", str(ROOT / "target/debug/isos")))
MOCK = ROOT / "tests/mock_mujoco_v2.py"
CONTRIBUTIONS = ROOT.parent / "isos-mujoco-sim/tools/node_contributions.py"


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
    build_stage = os.environ.get("ISOS_TEST_BUILD_STAGE") == "1"
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
        module_spec = importlib.util.spec_from_file_location("node_contributions", CONTRIBUTIONS)
        module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(module)
        manifest = bundle / "release.json"
        manifest.write_text(json.dumps({
            "schema_version": 4, "id": "island/mujoco-sim", "kind": "backend", "version": "2.0.0",
            "source_commit": "a" * 40, "node_contributions": module.contributions(),
            "assets": [{"os": "linux", "arch": "x86_64", "url": "https://github.com/Island-Team/isos-mujoco-sim/releases/download/v2.0.0/mock.tar.gz", "size_bytes": 1, "sha256": "b" * 64}],
        }))
        library = root / "mujoco/lib"
        library.mkdir(parents=True)
        (library / "libmujoco.so.3.16.0").write_bytes(b"test-only dependency stub")
        ui_repo = root / "ui"
        (ui_repo / "node_modules").mkdir(parents=True)
        (ui_repo / "en/isos").mkdir(parents=True)
        (ui_repo / "en/isos/index.html").write_text("<html><body>IsOS test workbench</body></html>")
        (ui_repo / "package.json").write_text(json.dumps({"name": "isos-ui-stub", "version": "2.0.0", "scripts": {"start": "python3 -m http.server ${ISOSUI_PORT} --bind 127.0.0.1"}}))
        isos_port = free_port()
        ui_port = free_port()
        env = dict(os.environ, ISOS_HOME=str(home), MUJOCO_ROOT=str(library.parent),
                   ISOS_HTTP_PORT=str(isos_port), ISOSUI_PORT=str(ui_port), ISOSUI_REPO_PATH=str(ui_repo),
                   ISOS_AETHER_BIN_DIR=str(EOS_BIN))
        if build_stage:
            built = subprocess.run([str(ISOS), "build", "eos"], env=env, text=True,
                                   capture_output=True, timeout=300)
            print("$ isos build eos\n" + built.stdout.strip(), flush=True)
            if built.stderr:
                print(built.stderr.strip(), flush=True)
            if built.returncode:
                raise RuntimeError(f"isos build eos exited {built.returncode}")
            assert (home / "data/builds/eos/current.json").is_file()
        run("pkg", "link", "island/mujoco-sim", "2.0.0", str(bundle), "--manifest", str(manifest), env=env)
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
        assert "hardware" not in graph["cores"]["eos"]["layers"]
        values_path = root / "node-values.json"
        values_path.write_text(json.dumps({"scene": "default", "socket": str(home / "run/default-mujoco.sock")}))
        graph = json.loads(run("pkg", "preview-set", "island/mujoco-sim", "2.0.0", "sim-hardware", str(graph_path), "--values", str(values_path), env=env).stdout)["graph"]
        hardware_groups = graph["cores"]["eos"]["layers"]["hardware"]["groups"]
        mujoco_nodes = {kind: hardware_groups[kind]["nodes"][name] for kind, name in
                        [("camera", "front"), ("base", "drive"), ("manipulator", "arm")]}
        assert all(node.get("package") is None for node in mujoco_nodes.values())
        assert all(node["adapter"]["package"] == {"id": "island/mujoco-sim", "version": "2.0.0"}
                   for node in mujoco_nodes.values())
        gate = graph["cores"]["eos"]["layers"]["interface"]["groups"]["gate"]["nodes"]["front-preview"]
        vision = graph["cores"]["isos"]["layers"]["ui"]["groups"]["panel"]["nodes"]["vision"]
        assert "package" not in gate and "package" not in vision
        backend = graph["cores"]["eos"]["layers"]["process"]["groups"]["mujoco"]["nodes"]["simulator"]["settings"]
        gui_report = root / "simulator-env.txt"
        parent_report = root / "simulator-parent.json"
        backend["env"] = {"MUJOCO_GUI": "1", "ISOS_TEST_ENV_REPORT": str(gui_report),
                          "ISOS_TEST_PARENT_REPORT": str(parent_report)}
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
        legacy_graph = json.loads(graph_path.read_text())
        legacy_backend = legacy_graph["cores"]["eos"]["layers"].pop("process")
        legacy_graph["cores"]["isos"]["layers"]["process"]["groups"]["backend"] = legacy_backend["groups"]["mujoco"]
        legacy_graph["cores"]["isos"]["layers"]["process"]["groups"]["backend"]["nodes"]["simulator"]["node_type"] = "isos.process.backend@v1.0"
        legacy_path = root / "legacy.nodes.json"
        legacy_path.write_text(json.dumps(legacy_graph))
        assert run("config", "validate", str(legacy_path), env=env, expect=False).returncode != 0
        run("config", "apply", str(graph_path), env=env)
        assert run("pkg", "remove", "island/mujoco-sim", "2.0.0", env=env, expect=False).returncode != 0
        saved_hash = hashlib.sha256(graph_path.read_bytes()).hexdigest()
        try:
            def isos_api(path, method="GET", data=None, timeout=4):
                payload = None if data is None else json.dumps(data).encode()
                request = Request(f"http://127.0.0.1:{isos_port}/api/isos/v2/{path}",
                                  data=payload, method=method,
                                  headers={"Origin": f"http://127.0.0.1:{isos_port}",
                                           "Content-Type": "application/json"})
                with urlopen(request, timeout=timeout) as response:
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
            if build_stage:
                built = isos_api("build-binaries", "POST", {"schema_version": 2, "target": "eos", "install_deps": False}, timeout=300)
                receipt = built["receipt"]
                assert receipt["component"] == "eos" and len(receipt["binaries"]) == 5
                assert isos_api("build-binaries")["selected"]["sha256"] == receipt["sha256"]
                assert built["suggested_graph"]["revision"] == saved_hash
                for node, binary in [("eos", "aether-runtime"), ("bridge", "aether-http-bridge"), ("config", "aether-config-server")]:
                    command = graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"][node]["settings"]["command"]
                    assert command[0] == receipt["binaries"][binary]
                print("BUILD CLI/API staged exact same EOS binary set: PASS", flush=True)
            for _ in range(200):
                try:
                    with urlopen(f"http://127.0.0.1:{ui_port}/en/isos", timeout=2) as response:
                        assert b"<html" in response.read().lower()
                    break
                except OSError:
                    if init_process.poll() is not None:
                        raise RuntimeError("IsOS init exited before npm frontend was ready")
                    time.sleep(0.05)
            else:
                raise RuntimeError("npm frontend did not become ready")
            try:
                urlopen(f"http://127.0.0.1:{isos_port}/en/isos", timeout=4)
            except HTTPError as error:
                assert error.code == 404
            else:
                raise AssertionError("backend unexpectedly served frontend")
            print("ISOS npm frontend and API split: PASS")
            assert isos_api("backends")["backends"][0]["scenes"] == ["default", "navigation"]
            assert isos_api("setup-status")["ready"]
            assert isos_api("node-graph/validate", "POST", {"schema_version": 2, "graph": document["graph"]})["valid"]
            try:
                isos_api("node-graph", "PUT", {"schema_version": 2, "revision": "stale", "graph": document["graph"]})
            except HTTPError as error:
                assert error.code == 409
            else:
                raise AssertionError("stale graph revision was accepted")
            applied = isos_api("node-graph", "PUT", {"schema_version": 2, "revision": saved_hash, "graph": document["graph"]})
            saved_hash = applied["revision"]
            assert hashlib.sha256(graph_path.read_bytes()).hexdigest() == saved_hash
            print("ISOS GRAPH UI API validation, revision conflict, and apply: PASS")
            startups = []
            run_log = (root / "run.log").open("w+")
            start = time.perf_counter()
            run_process = subprocess.Popen([str(ISOS), "run"], env=env,
                                           stdout=run_log, stderr=subprocess.STDOUT, text=True)
            for _ in range(200):
                if isos_api("process-status")["state"] == "running":
                    break
                if run_process.poll() is not None:
                    run_log.flush()
                    logs_dir = home / "data/process-logs/default"
                    child_logs = {path.name: path.read_text() for path in logs_dir.glob("*.log")}
                    raise RuntimeError(f"isos run exited {run_process.returncode}: {(root / 'run.log').read_text()}\nchild logs: {child_logs}")
                time.sleep(0.05)
            else:
                raise RuntimeError("isos run did not start")
            startups.append((time.perf_counter() - start) * 1000)
            for _ in range(4):
                start = time.perf_counter()
                result = isos_api("process-control", "POST", {"schema_version": 2, "operation": "restart"})
                assert result["started"][0] == "eos"
                startups.append((time.perf_counter() - start) * 1000)
            status = json.loads(run("status", "--json", env=env).stdout)
            assert status["state"] == "running" and status["package"]["version"] == "2.0.0"
            assert {p["id"] for p in status["processes"]} == {"eos", "bridge", "config"}
            assert json.loads(parent_report.read_text())["parent_pid"] == next(
                p["pid"] for p in status["processes"] if p["id"] == "eos")
            assert gui_report.read_text() == "1", "backend GUI setting did not reach the simulator"
            assert hashlib.sha256(graph_path.read_bytes()).hexdigest() == saved_hash, "EOS changed applied graph"
            browser_origin = f"http://127.0.0.1:{ui_port}"
            preflight = Request(f"http://127.0.0.1:{bridge_port}/api/eos/v2/control",
                                method="OPTIONS", headers={"Origin": browser_origin,
                                "Access-Control-Request-Method": "POST",
                                "Access-Control-Request-Headers": "content-type"})
            with urlopen(preflight, timeout=3) as response:
                assert response.status == 204
                assert response.headers["Access-Control-Allow-Origin"] == browser_origin
            browser_request = Request(f"http://127.0.0.1:{bridge_port}/api/eos/v2/control",
                                      data=b'{"op":"time"}',
                                      headers={"Origin": browser_origin,
                                               "Content-Type": "application/json"})
            with urlopen(browser_request, timeout=3) as response:
                assert response.headers["Access-Control-Allow-Origin"] == browser_origin
                assert json.load(response)["ok"]
            discovered = control({"op": "discover"}, bridge_port)
            assert len(discovered["nodes"]) >= 3
            camera = "eos/hardware/camera/front"
            base = "eos/hardware/base/drive"
            arm = "eos/hardware/manipulator/arm"
            frame = control({"op": "capture", "node_path": camera}, bridge_port)
            assert frame["frame"]["width_px"] == 1
            assert control({"op": "observe", "node_path": base}, bridge_port)["observation"]["valid"]
            assert len(control({"op": "observe", "node_path": arm}, bridge_port)["observation"]["joint_positions_rad"]) == 6
            now = control({"op": "time"}, bridge_port)["monotonic_ns"]
            base_cmd = {"op": "dispatch", "node_path": base, "command_id": "mock-base-1",
                        "deadline_monotonic_ns": now + 5_000_000_000,
                        "movement": {"type": "body_velocity", "x_m_s": 0.1, "y_m_s": 0.0,
                                     "yaw_rad_s": 0.0, "valid_for_ms": 100}}
            base_result = control(base_cmd, bridge_port)["result"]
            assert base_result["status"] != "Rejected", base_result
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
            run("logs", "eos", "--lines", "5", env=env)
            run_process.send_signal(signal.SIGINT)
            assert run_process.wait(timeout=10) == 0
            run_log.flush()
            run_output = (root / "run.log").read_text()
            assert f"Active revision: {saved_hash}" in run_output
            assert f"IsOS API: http://127.0.0.1:{isos_port}" in run_output
            assert f"IsosUI: http://127.0.0.1:{ui_port}/en/isos" in run_output
            assert f"IsOS log: {home / 'data/logs/default/isos.log'}" in run_output
            print("ISOS RUN Ctrl+C stopped managed children: PASS")
            assert json.loads(run("status", "--json", env=env).stdout)["state"] == "stopped"
            assert isos_api("process-status")["state"] == "stopped"
            original_graph = graph_path.read_bytes()
            stale_runtime = root / "stale-aether-runtime"
            stale_runtime.write_text("#!/bin/sh\necho 'unknown option: --capabilities' >&2\nexit 2\n")
            stale_runtime.chmod(0o700)
            stale_graph = json.loads(original_graph)
            stale_graph["cores"]["isos"]["layers"]["process"]["groups"]["component"]["nodes"]["eos"]["settings"]["command"][0] = str(stale_runtime)
            graph_path.write_text(json.dumps(stale_graph))
            try:
                isos_api("process-control", "POST", {"schema_version": 2, "operation": "run"})
                raise AssertionError("stale EOS runtime was accepted")
            except HTTPError as error:
                assert error.code == 422
                failure = json.load(error)
                assert ("capabilities" in failure["error"] and "isos build eos" in failure["error"]) or "staged Rust v2 build" in failure["error"]
            assert isos_api("process-status")["state"] == "stopped"
            graph_path.write_bytes(original_graph)
            print("FAILURE stale EOS binary returned API 422 without degraded startup: PASS")
            api_run = isos_api("process-control", "POST", {"schema_version": 2, "operation": "run", "revision": saved_hash})
            assert api_run["started"][0] == "eos"
            assert isos_api("process-logs/eos?lines=5")["node"] == "eos"
            categorized = isos_api("logs?source=eos&limit=5")
            assert isinstance(categorized["lines"], list) and isinstance(categorized["next_cursor"], str)
            assert isos_api("logs?source=ui&limit=5")["lines"]
            mujoco_lines = isos_api("logs?source=mujoco&limit=5")["lines"]
            assert any("mock mujoco ready" in line["message"] for line in mujoco_lines)
            assert isos_api("process-control", "POST", {"schema_version": 2, "operation": "stop"})["stopped"][-1] == "eos"
            print("ISOS HTTP Home, graph validation, process control, status, and logs: PASS")
            missing.write_text('<mujoco><mesh file="gone.stl"/></mujoco>')
            assert run("run", env=env, expect=False).returncode != 0
            failed_status = json.loads(run("status", "--json", env=env).stdout)
            assert "gone.stl" in failed_status["last_error"]
            missing.write_text("<mujoco/>")
            print("FAILURE missing linked asset blocked run and recorded last_error: PASS")
            assert isos_api("process-control", "POST", {"schema_version": 2, "operation": "run"})["started"][0] == "eos"
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
            init_exit = init_process.wait(timeout=10)
            init_log.flush()
            if init_exit != 0:
                raise AssertionError(f"isos init exited {init_exit}:\n{(root / 'init.log').read_text()}")
            for pid in shutdown_pids:
                try:
                    os.kill(pid, 0)
                except ProcessLookupError:
                    continue
                raise AssertionError(f"managed child {pid} survived init Ctrl+C")
            assert not (home / "run/manager.sock").exists()
            init_log.close()
            init_output = (root / "init.log").read_text()
            assert f"http://127.0.0.1:{ui_port}/en/isos" in init_output
            assert f"IsOS API: http://127.0.0.1:{isos_port}" in init_output
            assert f"IsOS log: {home / 'data/logs/default/isos.log'}" in init_output
            assert "#token=" not in init_output
            assert not (home / "run/ui-token").exists()
            with socket.socket() as probe:
                probe.settimeout(0.2)
                assert probe.connect_ex(("127.0.0.1", ui_port)) != 0
            print("ISOS INIT Ctrl+C stopped API and npm frontend: PASS")


if __name__ == "__main__":
    main()
