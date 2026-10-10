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

    def test_catalog_reference_needs_no_production_catalog_or_ingest(self):
        import importlib
        with mock.patch.dict(sys.modules):
            for name in ("tests.reference.catalog", "tests.reference.ingest"):
                sys.modules.pop(name, None)
            sys.modules["emojikit.catalog"] = None
            sys.modules["emojikit.ingest"] = None
            reference = importlib.import_module("tests.reference.catalog")
            self.assertEqual(reference.Catalog.__module__, "tests.reference.catalog")
            self.assertEqual(reference.catalog_identity.__module__, "tests.reference.ingest")
            from emojikit.errors import MediaError
            self.assertIs(sys.modules["tests.reference.ingest"].MediaError, MediaError)

    def test_migration_reference_needs_no_production_migration_or_catalog(self):
        import importlib
        with mock.patch.dict(sys.modules):
            for name in ("catalog", "ingest", "migration_bundle", "collection_migrate", "collection_state", "identity_repair"):
                sys.modules.pop("tests.reference." + name, None)
                sys.modules["emojikit." + name] = None
            reference = importlib.import_module("tests.reference.collection_migrate")
            repair = importlib.import_module("tests.reference.identity_repair")
            from emojikit.errors import MediaError
            self.assertIs(reference.MediaError, MediaError)
            self.assertIs(repair.cm, reference)
            self.assertEqual(repair.ROOT, ROOT)

    def test_panel_reference_needs_no_production_panel_or_catalog(self):
        import importlib
        with mock.patch.dict(sys.modules):
            names = ("panel", "panel_plan", "panel_view", "panel_save", "panel_instance", "panel_logging",
                     "panel_preview", "script_json", "catalog", "ingest", "collection_state")
            for name in names:
                sys.modules.pop("tests.reference." + name, None)
                sys.modules["emojikit." + name] = None
            panel = importlib.import_module("tests.reference.panel")
            plan = importlib.import_module("tests.reference.panel_plan")
            self.assertEqual(panel.ROOT, ROOT)
            self.assertEqual(panel.Catalog.__module__, "tests.reference.catalog")
            self.assertIs(panel.PlanError, plan.PlanError)

    def test_cli_references_need_no_production_builders_or_report(self):
        import importlib
        with mock.patch.dict(sys.modules):
            for name in ("build_pack", "make_emoji_pngs", "plan_status", "repaint_gate", "panel_plan"):
                sys.modules.pop("tests.reference." + name, None)
                sys.modules["emojikit." + name] = None
            for name in ("build_pack", "make_emoji_pngs", "plan_status"):
                reference = importlib.import_module("tests.reference." + name)
                self.assertEqual(reference.ROOT, ROOT)
                self.assertEqual(reference.__name__, "tests.reference." + name)

    def test_relocated_references_preserve_source_behavior_and_pinned_oracles(self):
        import hashlib
        import re
        class RelocationOnly(ast.NodeTransformer):
            def generic_visit(self, node):
                # Python 3.12 adds empty type_params to these same source nodes.
                node._fields = tuple(field for field in node._fields if field != "type_params")
                return super().generic_visit(node)
            def visit_Import(self, node):
                return None
            def visit_ImportFrom(self, node):
                return None
            def visit_Assign(self, node):
                if any(isinstance(target, ast.Name) and target.id in {"ROOT", "OFFSET_FILE"}
                       for target in node.targets):
                    return None
                return self.generic_visit(node)
        import json
        sources = json.loads((ROOT / "tests/reference/behavior.json").read_text(encoding="utf-8"))["sources"]
        references = {p.name for p in (ROOT / "tests/reference").glob("*.py")
                      if p.stem not in {"__init__", "identity_repair"}}
        self.assertEqual(set(sources), references)
        for name, evidence in sources.items():
            with self.subTest(reference=name):
                reference = ROOT / "tests/reference" / name
                new = RelocationOnly().visit(ast.parse(reference.read_text(encoding="utf-8")))
                digest = hashlib.sha256(ast.dump(new, include_attributes=False).encode("utf-8")).hexdigest()
                self.assertEqual(digest, evidence["relocated_behavior_sha256"])
                source = ROOT / evidence["source"]
                if source.exists():
                    self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), evidence["source_sha256"])
        ledger = (ROOT / "tests/oracles/README.md").read_text(encoding="utf-8")
        snapshots = re.findall(r"\| ([\w_]+\.py) \| ([0-9a-f]{64}) \|", ledger)
        self.assertEqual(len(snapshots), 20)
        for name, digest in snapshots:
            with self.subTest(oracle=name):
                self.assertEqual(hashlib.sha256((ROOT / "tests/oracles" / name).read_bytes()).hexdigest(), digest)

    def test_all_references_import_with_retired_production_modules_blocked(self):
        import subprocess
        code = (
            "import tests, importlib, pathlib, sys\n"
            "paths=sorted(pathlib.Path('tests/reference').glob('*.py'))\n"
            "names=[p.stem for p in paths if p.stem!='__init__']\n"
            "assert len(names)>=30, 'reference inventory unexpectedly empty'\n"
            "for name in names: sys.modules['emojikit.'+name]=None\n"
            "for name in names: importlib.import_module('tests.reference.'+name)\n"
            "assert not any(name.startswith('emojikit.') and name.split('.')[1] in names "
            "and module is not None for name,module in sys.modules.items())\n"
        )
        result = subprocess.run([sys.executable, "-c", code], cwd=ROOT,
            stdin=subprocess.DEVNULL, capture_output=True, encoding="utf-8", timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_shared_video_fixture_needs_no_retired_applications(self):
        import importlib
        with mock.patch.dict(sys.modules):
            sys.modules.pop("tests._video_fixtures", None)
            for name in ("catalog", "ingest", "add_media", "fetch_pack", "fetch_emoji_ids", "collection_reconcile"):
                sys.modules["emojikit." + name] = None
            fixture = importlib.import_module("tests._video_fixtures")
            self.assertEqual(len(fixture.RED), 100 * 100 * 4)
            self.assertEqual(fixture.encode.__module__, "tests._video_fixtures")

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
