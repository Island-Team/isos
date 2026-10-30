#!/usr/bin/env python3
"""Out-of-process MuJoCo v2 protocol stub for the clean-home test."""
import argparse
import base64
import json
import os
import socket
import struct

JPEG = base64.b64decode("/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcUFhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCAABAAEDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwD58ooor0TgP//Z")


def recv_exact(conn, size):
    chunks = bytearray()
    while len(chunks) < size:
        part = conn.recv(size - len(chunks))
        if not part:
            return None
        chunks.extend(part)
    return bytes(chunks)


def main():
    if os.environ.get("ISOS_TEST_ENV_REPORT"):
        with open(os.environ["ISOS_TEST_ENV_REPORT"], "w", encoding="utf-8") as report:
            report.write(os.environ.get("MUJOCO_GUI", ""))
    if os.environ.get("ISOS_TEST_PARENT_REPORT"):
        with open(os.environ["ISOS_TEST_PARENT_REPORT"], "w", encoding="utf-8") as report:
            json.dump({"pid": os.getpid(), "parent_pid": os.getppid()}, report)
    parser = argparse.ArgumentParser()
    for flag in ("socket", "benchmark-socket", "v2-socket", "scene"):
        parser.add_argument("--" + flag, required=True)
    args = parser.parse_args()
    path = args.v2_socket
    if os.path.exists(path):
        os.unlink(path)
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(path)
    os.chmod(path, 0o600)
    listener.listen(16)
    sequence = 0
    try:
        while True:
            conn, _ = listener.accept()
            with conn:
                header = recv_exact(conn, 8)
                if header is None:
                    continue
                metadata_len, payload_len = struct.unpack("<II", header)
                if not 0 < metadata_len <= 65536 or payload_len:
                    continue
                body = recv_exact(conn, metadata_len)
                if body is None:
                    continue
                request = json.loads(body)
                sequence += 1
                reply = {"version": "2.0.0", "id": request["id"], "ok": True,
                         "timestamp_ns": sequence * 1000000, "sequence": sequence}
                payload = b""
                op = request["op"]
                if op == "camera_capture":
                    reply.update(format="jpeg", rgb_length=len(JPEG), depth_available=False,
                                 width_px=1, height_px=1)
                    payload = JPEG
                elif op == "base_observe":
                    reply.update(position_m=[0.0, 0.0], linear_velocity_m_s=[0.0, 0.0],
                                 yaw_rad=0.0, angular_velocity_rad_s=0.0)
                elif op == "manipulator_observe":
                    reply.update(joint_positions_rad=[0.0] * 6, joint_velocities_rad_s=[0.0] * 6,
                                 end_position_m=[0.0, 0.0, 0.0], end_orientation_xyzw=[0.0, 0.0, 0.0, 1.0])
                elif op in ("base_move", "manipulator_move", "stop", "health"):
                    reply["accepted"] = True
                else:
                    reply.update(ok=False, error={"message": "unsupported operation"})
                encoded = json.dumps(reply, separators=(",", ":")).encode()
                conn.sendall(struct.pack("<II", len(encoded), len(payload)) + encoded + payload)
    finally:
        listener.close()
        if os.path.exists(path):
            os.unlink(path)


if __name__ == "__main__":
    main()
