"""Pinned live-resolution nomination and tri-state comparison expectations."""
from __future__ import annotations

import copy
import json
from pathlib import Path
from types import SimpleNamespace

from tests.backend_source_oracle import source_namespace, validate_cases

SOURCE = "d173116:collection_reconcile._near_catalog_match"
FIXTURE = Path(__file__).parent / "fixtures/backend/reconcile.json"


def fixtures():
    cases = []
    inputs = [
        ("animated-miss", "animated", None, [], {}),
        ("failed-raster-decode", "static", None, [], {}),
        ("distant-candidate-not-compared", "static", 0, [("s:red", 7)], {}),
        ("unhashed-candidate-examined", "static", 0, [("s:red", None)], {"s:red": True}),
        ("unique-color-match", "static", 0, [("s:red", 1), ("s:blue", 2)],
         {"s:red": True, "s:blue": False}),
        ("unreadable-rival-refuses", "static", 0, [("s:red", 1), ("s:blue", 2)],
         {"s:red": True, "s:blue": None}),
        ("two-proven-matches", "static", 0, [("s:red", 1), ("s:blue", 2)],
         {"s:red": True, "s:blue": True}),
        ("video-exact-digest-nominates-distant-hash", "video", 0, [("v:x:abc", 255)],
         {"v:x:abc": True}),
    ]
    for name, fmt, probe, rows, evidence in inputs:
        case = {"name": name, "fmt": fmt, "probe": probe, "lookup": "v:abc",
                "items": [{"content_key": key, "fmt": fmt, "phash": phash,
                           "file_path": key} for key, phash in rows],
                "comparison": copy.deepcopy(evidence)}
        calls = []
        class EvidencePath:
            def __init__(self, value):
                self.value = value
            def is_file(self):
                return True
        def compare(a, b, format, calls=calls, evidence=evidence):
            calls.append(a.value)
            return evidence[a.value]
        source = source_namespace("collection_reconcile.py", ("_near_catalog_match",), {
            "Path": EvidencePath, "AMBIGUOUS": "ambiguous", "UNDECIDABLE": "undecidable",
            "SEARCH_PHASH_TOLERANCE": 2,
            "identity": SimpleNamespace(perceptual_hash=lambda path, fmt, probe=probe: probe,
                content_key=lambda path, fmt: "v:abc", hamming=lambda a, b: (a ^ b).bit_count(),
                same_image=compare)})
        items = [SimpleNamespace(**item) for item in case["items"]]
        catalog = SimpleNamespace(all_items=lambda items=items: items)
        case["expected"] = source["_near_catalog_match"](catalog, "download", fmt)
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
        FIXTURE.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != document:
        raise SystemExit("Fixture drift: reconcile.json")
