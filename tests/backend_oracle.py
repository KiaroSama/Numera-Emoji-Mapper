"""Reproduce intent, resume, execution-step and view fixtures from pinned source."""

from __future__ import annotations

import datetime
import json
from pathlib import Path
from tests.backend_source_oracle import source_namespace, validate_cases

FIXTURE = Path(__file__).parent / "fixtures" / "backend" / "intent.json"
UTC = "2026-10-07T00:00:00Z"


class FrozenDatetime(datetime.datetime):
    @classmethod
    def now(cls, tz=None):
        return cls(2026, 10, 7, tzinfo=tz)


def intent_source():
    from types import SimpleNamespace

    clock = SimpleNamespace(datetime=FrozenDatetime, timezone=datetime.timezone)
    return source_namespace("panel_plan.py", (
        "PlanError", "build_plan", "target_map", "overlay_targets", "merge_plan"),
        {"_dt": clock})


def fixtures():
    source = intent_source()
    cases = [
        {"name": "empty", "operation": "build", "view": [], "targets": {}, "per_set": 200},
        {"name": "moves-held-and-unseen", "operation": "build", "view": [
            {"key": "a", "pack": 2, "included": True, "label": "😀"},
            {"key": "b", "pack": 3, "included": False},
            {"key": "c", "included": True},
            {"key": "logo", "pack": 1, "isLogo": True},
            {"key": "no-opinion", "pack": 4, "included": True}],
         "targets": {"a": 3, "b": 5, "c": 3, "logo": 2}, "per_set": 1},
        {"name": "scoped-merge-retains-unseen", "operation": "merge", "previous": {
            "version": 1, "targets": [["unseen", 9], ["a", 2]], "known": ["unseen", "a"],
            "excluded": [], "moves": [{"key": "unseen", "label": "u", "from_pack": 1,
                                       "to_pack": 9}], "held": [], "logo_slots": {"9": 1}},
         "view": [{"key": "a", "included": False, "pack": 2},
                  {"key": "new", "included": True}, {"key": "logo", "isLogo": True}],
         "targets": {"new": 9}, "scope": ["a", "new"], "per_set": 1, "live": None},
        {"name": "prune-only-with-catalog", "operation": "merge", "previous": {
            "version": 1, "targets": [["gone", 9], ["unseen", 2]], "known": ["gone", "unseen"],
            "excluded": ["gone"], "moves": [],
            "held": [{"key": "gone", "label": "gone", "from_pack": 9}],
            "logo_slots": {"9": 1, "2": 1}},
         "view": [{"key": "a", "pack": 1, "included": True},
                  {"key": "brand", "pack": 2, "isLogo": True}],
         "targets": {"a": 2}, "scope": ["a"], "per_set": 2, "live": ["a", "unseen"]},
        {"name": "legacy-explicit-moves", "operation": "targets", "previous": {
            "moves": [{"key": "a", "to_pack": 3}], "held": []}},
        {"name": "duplicate-target-refused", "operation": "targets", "previous": {
            "targets": [["a", 1], ["a", 2]], "moves": [], "held": []}},
        {"name": "overlay-does-not-rewrite-live-or-logo", "operation": "overlay",
         "view": [{"key": "a", "pack": 1, "included": True, "extra": {"kept": 1}},
                  {"key": "b", "pack": 2}, {"key": "logo", "pack": 1, "isLogo": True}],
         "previous": {"targets": [["a", 3], ["logo", 4]], "moves": [], "held": []}},
        {"name": "unicode-key-sort", "operation": "merge", "previous": None,
         "view": [{"key": k, "included": True} for k in ["é", "a", "😀", "中"]],
         "targets": {"é": 2, "a": 10, "😀": 1, "中": 2},
         "scope": ["é", "a", "😀", "中"], "per_set": 200, "live": []},
    ]
    for case in cases:
        args = {k: v for k, v in case.items() if k not in ("name", "operation")}
        operation = case["operation"]
        try:
            if operation == "build":
                value = source["build_plan"](**args)
            elif operation == "merge":
                args["scope"] = set(args["scope"])
                if args["live"] is not None:
                    args["live"] = set(args["live"])
                value = source["merge_plan"](**args)
            elif operation == "targets":
                value = source["target_map"](args["previous"])
            else:
                value = source["overlay_targets"](args["view"], args["previous"])
            case["expected"] = value
        except source["PlanError"] as exc:
            case["error"] = str(exc)
    document = {"source": "d173116:emojikit/panel_plan.py", "utc": UTC, "cases": cases}
    validate_cases(document, source=document["source"], count=8)
    return document


def resume_fixtures():
    import copy

    error = type("StateError", (RuntimeError,), {})
    namespace = source_namespace("collection_state.py", ("_validate_state",), {
        "Path": Path, "StateError": error, "FMT_TAG": {
            "static": "s", "video": "v", "animated": "a"},
        "MIXED": "mixed", "PER_SET": 200})
    base = {"base": "fixture", "sets": [{"name": "fixture1_by_YourEmojiBot", "fmt": "mixed",
             "index": 1, "keys": ["a"], "live": 2, "logo": True}],
            "sent": [], "sent_full": [], "skipped": []}
    inputs = [("defaults", {"base": "fixture", "sets": [{"name": "fixture1", "fmt": "static",
               "index": 1, "live": 0}], "sent": [], "sent_full": [], "skipped": []}),
              ("logo-membership", base),
              ("bool-index", dict(base, sets=[dict(base["sets"][0], index=True)])),
              ("missing-live", dict(base, sets=[dict(base["sets"][0], live=1)])),
              ("duplicate-key", dict(base, sets=[dict(base["sets"][0], keys=["a", "a"], live=3)])),
              ("wrong-format", dict(base, sets=[dict(base["sets"][0], fmt="bad")])),
              ("repeat-set", dict(base, sets=base["sets"] * 2)),
              ("empty-sent", dict(base, sent=[""]))]
    cases = []
    for name, state in inputs:
        case = {"name": name, "state": copy.deepcopy(state)}
        try:
            result = copy.deepcopy(state)
            namespace["_validate_state"](result, Path("fixture.json"))
            case["expected"] = result
        except error as exc:
            case["error"] = str(exc)
        cases.append(case)
    document = {"source": "d173116:collection_state._validate_state", "cases": cases}
    validate_cases(document, source=document["source"], count=8)
    return document


def generated_fixtures():
    from tests.backend_view_steps_oracle import steps_fixtures, view_fixtures

    return {"intent.json": fixtures(), "resume.json": resume_fixtures(),
            "steps.json": steps_fixtures(), "view.json": view_fixtures()}


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true",
                        help="Explicitly regenerate all four pinned-source fixtures.")
    args = parser.parse_args()
    for filename, data in generated_fixtures().items():
        target = FIXTURE.with_name(filename)
        if args.write:
            target.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n",
                              encoding="utf-8")
        elif json.loads(target.read_text(encoding="utf-8")) != data:
            raise SystemExit(f"Fixture drift: {target.name}; inspect the source/input delta")
