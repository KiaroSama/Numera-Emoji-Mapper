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
    def test_rgb_output_is_rebuilt_as_exact_rgba_then_reused(self):
        self.assertTrue(BINARY.is_file(), "native executable required")
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder).resolve() / "fixture-root"
            (root / "assets").mkdir(parents=True)
            shutil.copy2(ROOT / "assets/panel.html", root / "assets/panel.html")
            shutil.copy2(ROOT / "pyproject.toml", root / "pyproject.toml")
            shutil.copytree(ROOT / "emojikit", root / "emojikit", ignore=shutil.ignore_patterns("__pycache__"))
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
