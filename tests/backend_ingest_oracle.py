"""Source-derived identity decisions; comparisons are explicit boundary evidence."""
from __future__ import annotations

import copy
import json
from pathlib import Path
from types import SimpleNamespace

from tests._similarity_oracle import near_indices
from tests.backend_source_oracle import source_namespace, validate_cases

SOURCE = "d173116:ingest.catalog_identity"
FIXTURE = Path(__file__).parent / "fixtures/backend/ingest.json"


def fixtures():
    error = type("MediaError", (RuntimeError,), {})
    inputs = [
        {"name": "new", "items": [], "comparison": {}},
        {"name": "exact-key-is-not-equality", "items": ["s:abc"],
         "comparison": {"s:abc": False}},
        {"name": "collision-digest-match", "items": ["s:other:abc"],
         "comparison": {"s:other:abc": True}},
        {"name": "near-match", "items": ["s:different"],
         "comparison": {"s:different": True}},
        {"name": "unknown-beside-match", "items": ["s:abc", "s:different"],
         "comparison": {"s:abc": True, "s:different": None}},
        {"name": "two-matches", "items": ["s:abc", "s:different"],
         "comparison": {"s:abc": True, "s:different": True}},
        {"name": "occupied-collision", "items": ["s:abc", "s:collision:abc"],
         "comparison": {"s:abc": False, "s:collision:abc": False}},
        {"name": "disabled-phash", "items": ["s:different"],
         "comparison": {}, "threshold": -1},
    ]
    cases = []
    for row in inputs:
        case = copy.deepcopy(row)
        case["threshold"] = case.get("threshold", 2)
        case["incoming"] = {"content_key": "s:abc", "fmt": "static",
                            "file_path": "incoming", "phash": 0}
        case["items"] = [{"content_key": key, "fmt": "static", "file_path": key,
                          "phash": 1} for key in row["items"]]
        case["collision"] = "s:collision:abc"
        calls = []

        def compare(a, b, fmt, calls=calls, case=case):
            calls.append(str(a))
            return case["comparison"][str(a)]

        source = source_namespace("ingest.py", ("catalog_identity",), {
            "Path": Path, "MediaError": error, "near_indices": near_indices,
            "equivalent_files": compare,
            "identity": SimpleNamespace(collision_key=lambda path, lookup, case=case: case["collision"])})
        items = [SimpleNamespace(**item) for item in case["items"]]
        catalog = SimpleNamespace(phash_threshold=case["threshold"],
                                  all_items=lambda fmt, items=items: items,
                                  get=lambda key, items=items: next((it for it in items if it.content_key == key), None))
        try:
            case["expected"] = list(source["catalog_identity"](
                catalog, "s:abc", "static", Path("incoming"), 0))
        except error as exc:
            case["error"] = str(exc)
        case["calls"] = calls
        cases.append(case)
    document = {"source": SOURCE, "cases": cases}
    validate_cases(document, source=SOURCE, count=8)
    return document


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    document = fixtures()
    if args.write:
        FIXTURE.write_text(json.dumps(document, ensure_ascii=False, indent=2) + "\n",
                           encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != document:
        raise SystemExit("Fixture drift: ingest.json")
