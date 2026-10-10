"""Hash-verified AST slices of the exact pre-port source; no application imports."""
from __future__ import annotations

import ast
import hashlib
from pathlib import Path

BASELINE = "d173116"
ORACLES = Path(__file__).parent / "oracles"
SOURCE_HASHES = {
    "collection_preflight.py": "2821e30d219dbf4cb4df8ff08f97f697e6f89bf79cca1d5a2ae572000dc5d965",
    "migration_bundle.py": "8b2648956047e95fe5fcc1e779e85041366de917f1290faeb7b1d6c0e6e040c8",
    "collection_migrate.py": "4cb0211b690af6960a59e6c5ee851f64dc376aa611712fb938f6a87ed8ec8a68",
    "state_artifacts.py": "3b0faf872a3d7ee9ebfd8324060e7c2ff7541ead4b743fa30e5ebd9939248001",
    "pack_export.py": "1af5b4c013806130786b94130f31e00c92b6c6f2924e46c0f67e109fa39ee6f2",
    "pack_archive.py": "312e32d30bd0b8cdd905b93654a28b28131ce32f768fefd26e9d12aecd7f1c20",
    "status.py": "ee534afec67664ab7bea27499232df04fb0b9cbd80c97beb4259150dda7f3b7b",
    "pack_rows.py": "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121",
    "pack_gallery.py": "3a1460e846edb4bb75675f6d4d78d9e7d05ab75c7c61ec287d810a57cf5c9158",
    "script_json.py": "b9afcea858593c9e532ee3690038433e48eefc00910af603b6b1e8b90a3e56e4",
    "pack_manifest.py": "f335b1e8ce4f508a40c20d13cffca0cb6c8506fbeb9b0363b53acd094d86ec13",
    "emoji_bot.py": "e7ac5c65647d1e3e7b74e7fa8f7b027814439733aef1e1d0baab5033f53b175c",
    "collection_reconcile.py": "d22315d90e504d7acd622c02111eb9f2b9b3f33889fd483aa068cfe2b71d3dac",
    "ingest.py": "d2e7740602bd6993e51ab6fd0c0244764ca71e47b9dbafa2c6bdd5895050d0b6",
    "collection_state.py": "f8c6e96a1ea7c2fa34484ab5bd7c5cdcda4b7099d86f793acd4464e7284faf99",
    "packstate.py": "74a52b8a334a2d78f49d8cd26c9e60b566620f0ef398e31694433b6d461b7ac3",
    "panel_plan.py": "4c6b43a35ea5fa5907cc1933524b1a5c79214d12bb13cccc269e34d7ae38b1e4",
    "panel_view.py": "8935c34372dea19ec47d586369344390eecd44f0fc9b65a24f9581f13be5cd27",
    "plan_apply.py": "73da0b3c0e3a606f7bcdce49fcf1abddb4bc3c18fc6f59faa9344b739ccdefa3",
    "telegram_api.py": "59f5c958a2f7a2f7ed32b93669861066bf3a3f6a6a859623f7456405b5c56aa3",
}


def verify_sources(directory: Path = ORACLES) -> None:
    for name, expected in SOURCE_HASHES.items():
        actual = hashlib.sha256((directory / name).read_bytes()).hexdigest()
        if actual != expected:
            raise ValueError(f"Pinned oracle source drift: {BASELINE}:{name}")


def source_namespace(filename: str, names: tuple[str, ...], bindings: dict) -> dict:
    """Compile only named definitions after checking every pinned source file."""
    verify_sources()
    if filename not in SOURCE_HASHES:
        raise ValueError(f"Uninventoried oracle source: {filename}")
    path = ORACLES / filename
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    definitions = {node.name: node for node in tree.body
                   if isinstance(node, (ast.ClassDef, ast.FunctionDef))}
    missing = set(names) - definitions.keys()
    if missing or len(set(names)) != len(names):
        raise ValueError(f"Invalid oracle slice {filename}: {sorted(missing)}")
    namespace = {"__name__": __name__, **bindings}
    future = [node for node in tree.body
              if isinstance(node, ast.ImportFrom) and node.module == "__future__"]
    module = ast.Module(body=future + [definitions[name] for name in names], type_ignores=[])
    exec(compile(module, str(path), "exec", dont_inherit=True), namespace)
    return namespace


def validate_cases(document: dict, *, source: str, count: int) -> None:
    """Refuse an empty, duplicated or partially annotated parity corpus."""
    if document.get("source") != source:
        raise ValueError("Fixture source does not match its pinned seam")
    cases = document.get("cases")
    if not isinstance(cases, list) or len(cases) != count or not cases:
        raise ValueError(f"Expected {count} non-empty oracle cases")
    names = set()
    for case in cases:
        name = case.get("name")
        if not isinstance(name, str) or not name or name in names:
            raise ValueError("Oracle case names must be non-empty and unique")
        names.add(name)
        if ("expected" in case) == ("error" in case):
            raise ValueError(f"Oracle case {name} needs exactly one expected result or error")
        if "error" in case and (not isinstance(case["error"], str) or not case["error"]):
            raise ValueError(f"Oracle case {name} has no meaningful source error")
