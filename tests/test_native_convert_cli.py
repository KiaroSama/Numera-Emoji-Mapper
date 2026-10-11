"""Native conversion repairs non-RGBA output and matches the retained real-image codec."""
import contextlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from PIL import Image
from emojikit import media
from tests.test_native_local_cli import BINARY, ROOT

RUNS_ON_NATIVE_WINDOWS = True


class NativeConvertCLI(unittest.TestCase):
    def test_limit_and_failed_replacement_preserve_existing_output(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            shutil.copytree(ROOT / "emojikit", root / "emojikit", ignore=shutil.ignore_patterns("__pycache__"))
            # The repaired extension needs its sibling wheel libraries in a detached root.
            for libraries in ROOT.glob("*.libs"):
                if libraries.is_dir():
                    shutil.copytree(libraries, root / libraries.name)
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            source_dir, output = root / "input", root / "output"
            source_dir.mkdir()
            source = source_dir / "art.png"
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source)
            original = source.read_bytes()
            shutil.copy2(source, source_dir / "other.png")
            environment = {**os.environ, "PYO3_PYTHON": sys.executable}

            def run(*args):
                return subprocess.run([str(executable), "make-emoji-pngs", "--in", str(source_dir),
                                       "--out", str(output), *args], cwd=folder, env=environment,
                                      stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8",
                                      timeout=30, creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)

            invalid = run("--limit", "-1")
            self.assertEqual(invalid.returncode, 2, invalid.stderr)
            self.assertFalse(output.exists(), "invalid arguments must refuse before creating output")
            limited = run("--limit", "1")
            self.assertEqual(limited.returncode, 0, limited.stderr)
            self.assertEqual(sorted(path.name for path in output.glob("*.png")), ["art.png"])
            self.assertEqual(source.read_bytes(), original)
            prior = (output / "art.png").read_bytes()
            # A physical directory at the destination cannot be atomically replaced by a PNG.
            (output / "other.png").mkdir()
            protected = output / "other.png/keep.bin"
            protected.write_bytes(b"previous durable output")
            refused = run()
            self.assertEqual(refused.returncode, 2, refused.stderr)
            self.assertEqual(protected.read_bytes(), b"previous durable output")
            self.assertEqual((output / "art.png").read_bytes(), prior)
            self.assertEqual(source.read_bytes(), original)
            self.assertEqual((source_dir / "other.png").read_bytes(), original)
            self.assertEqual(list((output / ".native-codec").iterdir()), [], "failed staging must be reaped")

    @unittest.skipUnless(os.name == "nt", "Windows denies replacement of an opened file without delete sharing")
    def test_failed_same_file_replacement_keeps_previous_png_and_source(self):
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                      wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        kernel.CreateFileW.restype = wintypes.HANDLE
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.restype = wintypes.BOOL
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            source, output = folder / "source", folder / "output"
            source.mkdir()
            output.mkdir()
            original = source / "art.png"
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", original)
            target = output / "art.png"
            with Image.open(original) as image:
                image.convert("RGB").resize((100, 100)).save(target)
            before = target.read_bytes(), original.read_bytes()
            handle = kernel.CreateFileW(str(target), 0x80000000, 1 | 2, None, 3, 0, None)
            self.assertNotEqual(handle, ctypes.c_void_p(-1).value)
            try:
                result = subprocess.run([str(BINARY), "make-emoji-pngs", "--in", str(source), "--out", str(output)],
                    cwd=folder, env={**os.environ, "PYO3_PYTHON": sys.executable}, stdin=subprocess.DEVNULL,
                    capture_output=True, encoding="utf-8", timeout=30, creationflags=subprocess.CREATE_NO_WINDOW)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual((target.read_bytes(), original.read_bytes()), before)
                self.assertEqual(list((output / ".native-codec").iterdir()), [])
            finally:
                self.assertTrue(kernel.CloseHandle(handle))

    def test_competing_output_owner_refuses_before_touching_markers_or_png(self):
        from emojikit.packstate import exclusive_lock
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            folder = Path(folder).resolve()
            source = folder / "source"
            output = folder / "output"
            source.mkdir()
            output.mkdir()
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source / "art.png")
            prior = output / "art.png"
            prior.write_bytes(b"previous output awaiting repair")
            marker = output / ".svg_cur"
            marker.write_text("previous attempt", encoding="utf-8")
            before = {path: path.read_bytes() for path in (prior, marker, source / "art.png")}
            with exclusive_lock(output / ".convert.lock"):
                result = subprocess.run([str(BINARY), "make-emoji-pngs", "--in", str(source), "--out", str(output)],
                    cwd=folder, env={**os.environ, "PYO3_PYTHON": sys.executable},
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("conversion output is already owned", result.stderr)
            self.assertEqual({path: path.read_bytes() for path in before}, before)
            self.assertFalse((output / ".svg_skip.txt").exists())
            self.assertFalse((output / ".native-codec").exists())

    def test_rgb_output_is_rebuilt_as_exact_rgba_then_reused(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            shutil.copytree(ROOT / "emojikit", root / "emojikit", ignore=shutil.ignore_patterns("__pycache__"))
            # The repaired extension needs its sibling wheel libraries in a detached root.
            for libraries in ROOT.glob("*.libs"):
                if libraries.is_dir():
                    shutil.copytree(libraries, root / libraries.name)
            executable = root / BINARY.name
            shutil.copy2(BINARY, executable)
            source_dir, output = root / "input", root / "output"
            source_dir.mkdir()
            output.mkdir()
            source = source_dir / "art.png"
            shutil.copy2(ROOT / "assets/numera-emoji-mapper-logo.png", source)
            expected = media.to_static_png(source, root / "expected.png")
            target = output / "art.png"
            with Image.open(expected) as image:
                image.convert("RGB").save(target)
            os.utime(source, (1700000000, 1700000000))
            os.utime(target, (1700000060, 1700000060))
            environment = {**os.environ, "PYO3_PYTHON": sys.executable}
            def run(*args):
                options = list(args) or ["--in", str(source_dir), "--out", str(output)]
                return subprocess.run([str(executable), "make-emoji-pngs", *options],
                    cwd=folder, env=environment, stdin=subprocess.DEVNULL, capture_output=True,
                    encoding="utf-8", timeout=20,
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            from tests.reference import make_emoji_pngs
            source_output = root / "source-output"
            with contextlib.redirect_stdout(io.StringIO()) as captured:
                expected_code = make_emoji_pngs._run_general(source_dir, source_output, 0)
            expected_text = captured.getvalue().replace(str(source_output), str(output))
            result = run()
            self.assertEqual(result.returncode, expected_code, result.stderr)
            self.assertEqual(result.stdout, expected_text)
            with Image.open(target) as image:
                self.assertEqual(image.mode, "RGBA", "an RGB file is not a completed transparent emoji")
                self.assertEqual(image.size, (100, 100))
            self.assertEqual(target.read_bytes(), expected.read_bytes())
            from unittest import mock
            from scripts import native_convert_owner
            installed = root / "native/runtime" / executable.name
            installed.parent.mkdir(parents=True)
            shutil.copy2(executable, installed)
            with mock.patch.object(native_convert_owner, "ROOT", root):
                code = native_convert_owner.run(source_dir, output, root / "owned-output", root / "owned-errors")
            self.assertEqual(code, 0, (root / "owned-errors").read_text(encoding="utf-8"))
            self.assertIn("DONE:", (root / "owned-output").read_text(encoding="utf-8"))
            self.assertEqual(target.read_bytes(), expected.read_bytes())
            before = target.read_bytes(), target.stat().st_mtime_ns
            result = run()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((target.read_bytes(), target.stat().st_mtime_ns), before)
            self.assertFalse((output / ".svg_cur").exists())
            self.assertEqual(list((output / ".native-codec").iterdir()), [])
            logos = root / "coins/logos"
            (logos / "svg").mkdir(parents=True)
            (logos / "png").mkdir()
            (logos / "svg/art.svg").write_text("<svg broken", encoding="utf-8")
            shutil.copy2(source, logos / "png/art.png")
            # A killed SVG is quarantined beside the source folders; the PNG
            # fallback still succeeds and clears this stem's failed attempt.
            (logos / ".svg_cur").write_text("quarantined", encoding="utf-8")
            (logos / "svg/quarantined.svg").write_text("<svg broken", encoding="utf-8")
            shutil.copy2(source, logos / "png/quarantined.png")
            result = run("--limit", "0")
            self.assertEqual(result.returncode, 0, result.stderr)
            for name in ("art", "quarantined"):
                self.assertEqual((logos / f"emoji/{name}.png").read_bytes(), expected.read_bytes())
            self.assertFalse((logos / ".svg_cur").exists())
            self.assertIn("quarantined", (logos / ".svg_skip.txt").read_text(encoding="utf-8"))
            self.assertFalse((logos / "emoji/.svg_skip.txt").exists())
            shutil.rmtree(logos / "svg")
            result = run("--limit", "0")
            self.assertEqual(result.returncode, 0, result.stderr)
            for name in ("art", "quarantined"):
                self.assertEqual((logos / f"emoji/{name}.png").read_bytes(), expected.read_bytes())
            shutil.rmtree(logos)
            result = run("--limit", "0")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("made 0 this run", result.stdout)
            self.assertTrue((logos / "emoji").is_dir())
            if os.name == "nt":
                scripts = root / "scripts"
                scripts.mkdir()
                for name in ("native_convert_owner.py", "build_job.py"):
                    shutil.copy2(ROOT / "scripts" / name, scripts / name)
                watchdog = (ROOT / "coins/run_convert.ps1").read_text(encoding="utf-8")
                # Only the selected interpreter differs in this detached fixture.
                watchdog = watchdog.replace("Join-Path $ProjectRoot '.venv\\Scripts\\python.exe'",
                                            "'" + sys.executable.replace("'", "''") + "'")
                (root / "coins/run_convert.ps1").write_text(watchdog, encoding="utf-8")
                (logos / "svg").mkdir()
                (logos / "png").mkdir()
                (logos / "svg/vector.svg").write_text(
                    '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">'
                    '<rect width="100" height="100" fill="red"/></svg>', encoding="utf-8")
                shutil.copy2(source, logos / "png/art.png")
                result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-File",
                                         str(root / "coins/run_convert.ps1")], cwd=folder, env=environment,
                    stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=30,
                    creationflags=subprocess.CREATE_NO_WINDOW)
                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertIn("SVG conversion complete", result.stdout)
                self.assertEqual((logos / "emoji/art.png").read_bytes(), expected.read_bytes())
                self.assertTrue((logos / "emoji/vector.png").is_file())
                self.assertFalse((logos / "emoji/.svg_cur").exists())
                self.assertEqual(list((logos / "emoji/.native-codec").iterdir()), [])
