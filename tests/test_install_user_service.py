"""Exercise installer service opt-in without touching the user's systemd session."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SOURCE_ROOT = Path(__file__).resolve().parents[1]


class InstallUserServiceTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.checkout = self.root / "checkout"
        (self.checkout / "tools").mkdir(parents=True)
        (self.checkout / "target/release").mkdir(parents=True)
        for source in ("install.sh", "tools/user-service.sh"):
            destination = self.checkout / source
            shutil.copy2(SOURCE_ROOT / source, destination)
        self.write_executable(self.checkout / "build.sh", "#!/bin/sh\nexit 0\n")
        self.write_executable(
            self.checkout / "target/release/isos",
            '#!/bin/sh\nif [ "$1" = --version ]; then echo "isos 1.0.0"; fi\n',
        )
        fake_bin = self.root / "fake-bin"
        fake_bin.mkdir()
        self.write_executable(
            fake_bin / "cargo",
            '#!/bin/sh\n'
            'while [ "$#" -gt 0 ]; do\n'
            '  if [ "$1" = --root ]; then shift; root="$1"; fi\n'
            '  shift\n'
            'done\n'
            'mkdir -p "$root/bin"\n'
            'cp "$ISOS_TEST_BINARY" "$root/bin/isos"\n',
        )
        self.write_executable(
            fake_bin / "systemctl",
            '#!/bin/sh\n'
            'printf "%s\\n" "$*" >> "$ISOS_TEST_SYSTEMCTL_LOG"\n'
            'case "$*" in *is-active*) exit 3;; esac\n',
        )
        home = self.root / "home"
        (home / ".config/isos").mkdir(parents=True)
        (home / ".config/isos/mode").write_text("product\n")
        self.environment = dict(os.environ)
        self.environment.update(
            HOME=str(home),
            PATH=f"{fake_bin}:{os.environ['PATH']}",
            ISOS_INSTALL_ROOT=str(self.root / "installed"),
            ISOS_TEST_BINARY=str(self.checkout / "target/release/isos"),
            ISOS_TEST_SYSTEMCTL_LOG=str(self.root / "systemctl.log"),
        )

    @staticmethod
    def write_executable(path, content):
        path.write_text(content)
        path.chmod(0o755)

    def install(self, *arguments):
        return subprocess.run(
            [str(self.checkout / "install.sh"), *arguments],
            env=self.environment,
            text=True,
            capture_output=True,
            check=False,
        )

    def systemctl_calls(self):
        log = self.root / "systemctl.log"
        return log.read_text().splitlines() if log.exists() else []

    def test_default_install_does_not_enable_or_start_service(self):
        result = self.install()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.root / "installed/bin/isos").exists())
        self.assertEqual(self.systemctl_calls(), ["--user is-active --quiet isos-server.service"])
        self.assertFalse((self.root / "home/.config/systemd/user/isos.service").exists())

    def test_flag_installs_and_starts_user_service(self):
        result = self.install("--install-user-service")
        self.assertEqual(result.returncode, 0, result.stderr)
        unit = self.root / "home/.config/systemd/user/isos.service"
        self.assertIn(f"ExecStart={self.root}/installed/bin/isos serve", unit.read_text())
        self.assertEqual(
            self.systemctl_calls(),
            [
                "--user is-active --quiet isos-server.service",
                "--user daemon-reload",
                "--user enable --now isos.service",
            ],
        )

    def test_unknown_flag_exits_before_install(self):
        result = self.install("--unknown")
        self.assertEqual(result.returncode, 2)
        self.assertFalse((self.root / "installed").exists())
        self.assertEqual(self.systemctl_calls(), [])

    def test_active_obsolete_service_blocks_install_without_stopping_it(self):
        self.write_executable(
            self.root / "fake-bin/systemctl",
            '#!/bin/sh\nprintf "%s\\n" "$*" >> "$ISOS_TEST_SYSTEMCTL_LOG"\n'
            'case "$*" in *is-active*) exit 0;; esac\n',
        )
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("systemctl --user stop isos-server.service", result.stderr)
        self.assertEqual(self.systemctl_calls(), ["--user is-active --quiet isos-server.service"])
        self.assertFalse((self.root / "installed/bin/isos").exists())


if __name__ == "__main__":
    unittest.main()
