"""Retained development launchers must not import retired production applications."""
import ast
from pathlib import Path
import sys
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent
RUNS_ON_NATIVE_WINDOWS = True


class NativeConsumerBoundary(unittest.TestCase):
    def test_converter_watchdog_passes_use_the_installed_native_command(self):
        from scripts import native_convert_owner
        import tempfile
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder)
            executable = root / "native/runtime" / ("numera-emoji.exe" if sys.platform == "win32" else "numera-emoji")
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b"fixture executable, never launched")
            child = mock.Mock()
            child.wait.return_value = 3
            child.poll.return_value = 3
            with mock.patch.object(native_convert_owner, "ROOT", root), \
                    mock.patch.object(native_convert_owner.subprocess, "Popen", return_value=child) as spawn, \
                    mock.patch("scripts.build_job.Job") as job, \
                    mock.patch.object(native_convert_owner.os, "killpg", create=True):
                self.assertEqual(native_convert_owner.run(root / "in with spaces", root / "out",
                                 root / "stdout", root / "stderr"), 3)
            command = spawn.call_args.args[0]
            self.assertEqual(command, [str(executable), "make-emoji-pngs", "--in", str(root / "in with spaces"), "--out", str(root / "out")])
            child.wait.assert_any_call(timeout=10)
            if sys.platform == "win32":
                job.return_value.attach.assert_called_once_with(child)
                job.return_value.finish.assert_called_once()
        script = (ROOT / "coins/run_convert.ps1").read_text(encoding="utf-8")
        self.assertNotIn("emojikit.make_emoji_pngs", script)
        self.assertNotIn("emojikit\\make_emoji_pngs.py", script)

    def test_converter_idle_timeout_stops_owned_tree_without_waiting_minutes(self):
        import subprocess
        import tempfile
        from scripts import native_convert_owner
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            root = Path(folder)
            executable = root / "native/runtime" / ("numera-emoji.exe" if sys.platform == "win32" else "numera-emoji")
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b"fixture executable, never launched")
            child = mock.Mock()
            child.wait.side_effect = [subprocess.TimeoutExpired("fixture", 2), 0]
            child.poll.return_value = 0
            with mock.patch.object(native_convert_owner, "ROOT", root), \
                    mock.patch.object(native_convert_owner.subprocess, "Popen", return_value=child), \
                    mock.patch.object(native_convert_owner.time, "monotonic", side_effect=[0, 121]), \
                    mock.patch("scripts.build_job.Job") as job, \
                    mock.patch.object(native_convert_owner.os, "killpg", create=True):
                self.assertEqual(native_convert_owner.run(root / "in", root / "out",
                                 root / "stdout", root / "stderr"), 124)
            child.wait.assert_any_call(timeout=10)
            if sys.platform == "win32":
                job.return_value.finish.assert_called_once()

    def test_relocated_references_preserve_source_behavior_and_pinned_oracles(self):
        import hashlib
        import re
        from tests._reference_behavior import digest
        import json
        sources = json.loads((ROOT / "tests/reference/behavior.json").read_text(encoding="utf-8"))["sources"]
        references = {p.name for p in (ROOT / "tests/reference").glob("*.py")
                      if p.stem not in {"__init__", "identity_repair"}}
        self.assertEqual(set(sources), references)
        for name, evidence in sources.items():
            with self.subTest(reference=name):
                reference = ROOT / "tests/reference" / name
                self.assertEqual(digest(reference.read_text(encoding="utf-8")), evidence["relocated_behavior_sha256"])
                source = ROOT / evidence["source"]
                if source.exists():
                    self.assertEqual(hashlib.sha256(source.read_text(encoding="utf-8").encode("utf-8")).hexdigest(),
                                     evidence["source_sha256"])
        ledger = (ROOT / "tests/oracles/README.md").read_text(encoding="utf-8")
        snapshots = re.findall(r"\| ([\w_]+\.py) \| ([0-9a-f]{64}) \|", ledger)
        self.assertEqual(len(snapshots), 20)
        for name, expected_digest in snapshots:
            with self.subTest(oracle=name):
                self.assertEqual(hashlib.sha256((ROOT / "tests/oracles" / name).read_bytes()).hexdigest(), expected_digest)

    def test_all_references_import_with_retired_production_modules_blocked(self):
        import subprocess
        code = (
            "import tests, importlib, pathlib, sys\n"
            "paths=sorted(pathlib.Path('tests/reference').glob('*.py'))\n"
            "names=[p.stem for p in paths if p.stem!='__init__']\n"
            "assert len(names)>=30, 'reference inventory unexpectedly empty'\n"
            "for name in names: sys.modules['emojikit.'+name]=None\n"
            "for name in names: importlib.import_module('tests.reference.'+name)\n"
            "root=pathlib.Path.cwd()\n"
            "for name in names:\n"
            " module=sys.modules['tests.reference.'+name]\n"
            " if hasattr(module,'ROOT'): assert module.ROOT==root, name\n"
            "from tests.reference import panel,panel_plan,identity_repair,collection_migrate,ingest\n"
            "from emojikit.errors import MediaError\n"
            "assert panel.PlanError is panel_plan.PlanError\n"
            "assert identity_repair.cm is collection_migrate\n"
            "assert ingest.MediaError is MediaError\n"
            "import tests._video_fixtures as fixture\n"
            "assert len(fixture.RED)==100*100*4\n"
            "assert not any(name.startswith('emojikit.') and name.split('.')[1] in names "
            "and module is not None for name,module in sys.modules.items())\n"
        )
        result = subprocess.run([sys.executable, "-c", code], cwd=ROOT,
            stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_sandbox_imports_without_the_python_panel_application(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location("sandbox_boundary", ROOT / "scripts/panel_sandbox.py")
        module = importlib.util.module_from_spec(spec)
        with mock.patch.dict(sys.modules, {"emojikit.panel": None}):
            spec.loader.exec_module(module)
        self.assertEqual(module.PANEL_PORT, 9450)
        tree = ast.parse((ROOT / "scripts/panel_sandbox.py").read_text(encoding="utf-8"))
        self.assertFalse(any(isinstance(node, ast.ImportFrom) and node.module == "tests.reference"
                             for node in ast.walk(tree)))
