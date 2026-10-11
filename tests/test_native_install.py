"""Native executable installs independently of Cargo cache and preserves prior bytes on failure."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

from scripts import build_native
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeInstall(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "PowerShell launcher boundary is Windows-only")
    def test_installed_launcher_probe_and_refusal_work_without_cargo(self):
        import sys
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve() / "package with spaces"
            (root / "native/runtime").mkdir(parents=True)
            (root / "assets").mkdir()
            (root / "scripts").mkdir()
            shutil.copy2(BINARY, root / "native/runtime" / BINARY.name)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            shutil.copy2(ROOT / "scripts/run-native.ps1", root / "scripts/run-native.ps1")
            # This boundary fixture supplies only the verified executable route;
            # the extracted installer/launch_prebuilt environment has its own real acceptance.
            (root / "scripts/launch_prebuilt.py").write_text(
                "import subprocess,sys\nfrom pathlib import Path\n"
                "raise SystemExit(subprocess.call([str(Path(__file__).resolve().parents[1]/'native/runtime/numera-emoji.exe'),*sys.argv[1:]]))\n",
                encoding="utf-8")
            script = root / "probe.ps1"
            script.write_text("param([string]$Python)\n$ScriptRoot=$PSScriptRoot\n"
                "function Write-Err($m){[Console]::Error.WriteLine($m)}\n"
                "function Invoke-Py($py,[string[]]$Argv){& $py @Argv; return [int]$LASTEXITCODE}\n"
                ". (Join-Path $ScriptRoot 'scripts/run-native.ps1')\n"
                "if(-not(Test-NativeRuntime $Python)){exit 1}\nexit 0\n", encoding="utf-8")
            command = ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-File", str(script), sys.executable]
            result = subprocess.run(command, cwd=folder, capture_output=True, encoding="utf-8",
                stdin=subprocess.DEVNULL, timeout=15, creationflags=subprocess.CREATE_NO_WINDOW)
            self.assertEqual(result.returncode, 0, result.stderr)
            (root / "native/runtime" / BINARY.name).unlink()
            refused = subprocess.run(command, cwd=folder, capture_output=True, encoding="utf-8",
                stdin=subprocess.DEVNULL, timeout=15, creationflags=subprocess.CREATE_NO_WINDOW)
            self.assertEqual(refused.returncode, 1)
            self.assertIn("Installed native executable missing", refused.stderr)
            self.assertFalse((root / "native/target").exists())

    def test_checkout_extension_keeps_repaired_wheel_libraries(self):
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder) / "checkout"
            package = Path(folder) / "installed/emojikit"
            package.mkdir(parents=True)
            source = package / "_native.abi3.so"
            source.write_bytes(b"fixture extension")
            libraries = package.parent / "numera_emoji_core.libs"
            libraries.mkdir()
            (libraries / "libssl-fixture.so").write_bytes(b"fixture shared dependency")
            with mock.patch.object(build_native, "ROOT", root):
                destination = build_native.install_extension(source)
            self.assertEqual(destination.read_bytes(), source.read_bytes())
            self.assertEqual((root / libraries.name / "libssl-fixture.so").read_bytes(),
                             b"fixture shared dependency")

    def test_installed_binary_runs_after_source_removed_and_failed_replace_preserves_it(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve() / "root with spaces"
            (root / "native/target/release").mkdir(parents=True)
            source = root / "native/target/release" / BINARY.name
            shutil.copy2(BINARY, source)
            with mock.patch.object(build_native, "ROOT", root):
                installed = build_native.install_executable(source)
                self.assertEqual(installed.parent, root / "native/runtime")
                self.assertEqual(installed.read_bytes(), source.read_bytes())
                source.unlink()
                result = subprocess.run([str(installed), "--help"], cwd=folder,
                    capture_output=True, encoding="utf-8", stdin=subprocess.DEVNULL, timeout=5,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("identity-repair", result.stdout)
                (root / "assets").mkdir()
                shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
                shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
                result = subprocess.run([str(installed), "status"], cwd=folder,
                    capture_output=True, encoding="utf-8", stdin=subprocess.DEVNULL, timeout=5,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertIn("STALE roster", result.stdout)
                self.assertFalse((root / "collection/catalog.db").exists())
                logs = list((root / "logs").glob("status_*_UTC_*.log"))
                self.assertEqual(len(logs), 1)
                text = logs[0].read_text(encoding="utf-8")
                self.assertIn("[runtime] native backend started", text)
                self.assertIn("exit=3", text)
                before = installed.read_bytes()
                source.write_bytes(b"replacement bytes")
                with mock.patch.object(build_native.os, "replace", side_effect=PermissionError("fixture occupied")):
                    with self.assertRaises(PermissionError):
                        build_native.install_executable(source)
                self.assertEqual(installed.read_bytes(), before)
                self.assertEqual(list(installed.parent.iterdir()), [installed])
