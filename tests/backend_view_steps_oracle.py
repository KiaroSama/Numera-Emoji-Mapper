"""Reproducible view/step inputs and projections, without native expected values."""
from __future__ import annotations

import copy
from dataclasses import asdict, dataclass, field
from pathlib import Path
import re
from types import SimpleNamespace

from tests.backend_oracle import intent_source
from tests.backend_source_oracle import source_namespace, validate_cases

STEPS_SOURCE = "d173116:plan_apply.compute_steps"
VIEW_SOURCE = "d173116:panel_view.build_view"


def steps_inputs():
    plan = {"targets": [["a", 2], ["c", 1]], "moves": [], "held": [],
            "excluded": [], "per_set": 200}
    state = {"sets": [
        {"fmt": "mixed", "index": 1, "keys": ["a", "b"], "logo": True},
        {"fmt": "mixed", "index": 2, "keys": [], "logo": True}], "skipped": []}
    items = {"a": {"included": True, "pos": 1}, "b": {"included": True, "pos": 0},
             "c": {"included": True, "pos": 2}}
    case = {"name": "move-new-reorder", "plan": plan, "state": state,
            "items": items, "ids": {"a": "111111111"}}
    cases = [copy.deepcopy(case) for _ in range(6)]
    cases[1]["name"] = "held-live"
    cases[1]["plan"]["excluded"] = ["a"]
    cases[2]["name"] = "missing-id"
    cases[2]["ids"] = {}
    cases[3]["name"] = "capacity-refusal"
    cases[3]["plan"]["per_set"] = 1
    cases[4]["name"] = "next-pack"
    cases[4]["plan"]["targets"] = [["c", 3]]
    cases[5]["name"] = "gap-refusal"
    cases[5]["plan"]["targets"] = [["c", 4]]
    return cases


def steps_fixtures():
    intent = intent_source()
    source = source_namespace("plan_apply.py", ("Refusal", "Steps", "compute_steps"), {
        "dataclass": dataclass, "field": field, "MIXED": "mixed", "PER_SET": 200,
        "target_map": intent["target_map"]})
    cases = steps_inputs()
    for case in cases:
        try:
            result = source["compute_steps"](**{
                key: copy.deepcopy(case[key]) for key in ("plan", "state", "items", "ids")})
            expected = asdict(result)
            expected["removals"] = [list(row) for row in result.removals]
            expected["adds"] = {str(key): value for key, value in result.adds.items()}
            expected["moved_in"] = sorted(result.moved_in)
            expected["held_live"] = [list(row) for row in result.held_live]
            expected["counted"], expected["pending"] = result.counted, result.pending
            case["expected"] = expected
        except (source["Refusal"], intent["PlanError"]) as exc:
            case["error"] = str(exc)
    document = {"source": STEPS_SOURCE, "cases": cases}
    validate_cases(document, source=STEPS_SOURCE, count=6)
    return document


def view_inputs():
    items = [
        {"content_key": "s:a", "fmt": "static", "file_path": "a.png", "emojis": ["😀"],
         "keywords": ["premium-id:123"], "sources": [], "phash": 0,
         "custom_emoji_id": None, "uploaded": False, "included": True},
        {"content_key": "s:b", "fmt": "video", "file_path": "b.webm", "emojis": [],
         "keywords": [], "sources": [], "phash": None, "custom_emoji_id": None,
         "uploaded": True, "included": False},
        {"content_key": "s:c", "fmt": "animated", "file_path": "c.tgs", "emojis": ["✅"],
         "keywords": [], "sources": [], "phash": None, "custom_emoji_id": None,
         "uploaded": False, "included": True}]
    request = {"items": items, "published": ["s:b", "s:c"],
               "published_sets": {"s:b": "fixture1", "s:c": "fixture2"},
               "keep_sets": {}, "show_published": False, "branded": False, "logo_path": ""}
    cases = [{"name": name, "request": copy.deepcopy(request)}
             for name in ("hide-live", "show-all", "selected-pack")]
    cases[1]["request"]["show_published"] = True
    cases[2]["request"]["keep_sets"] = {"fixture1": 2}
    return cases


class ViewCatalog:
    """A read-only catalog projection; omitted seed behavior cannot pass silently."""

    def __init__(self, request):
        self.request = request
        self.calls = []

    def all_items(self):
        self.calls.append("all_items")
        return [SimpleNamespace(**item) for item in self.request["items"]]

    def published_keys(self):
        self.calls.append("published_keys")
        return set(self.request["published"])

    def published_set_names(self):
        self.calls.append("published_set_names")
        return self.request["published_sets"]


def view_fixtures():
    # Current fixtures cover seed_order=False/unbranded only. No native function
    # or production operator configuration may be consulted to generate them.
    configuration = SimpleNamespace(brand_logo_path=lambda **kwargs: None,
                                    brand_logo_bots=lambda **kwargs: frozenset())
    source = source_namespace("panel_view.py", ("copy_id_for", "build_view"), {
        "Path": Path, "_PREMIUM_ID": re.compile(r"^premium-id:(\d+)$"),
        "operator_config": configuration, "LOGO_KEY": "__brand_logo__"})
    cases = view_inputs()
    for case in cases:
        request = case["request"]
        if request["branded"] or request["logo_path"]:
            raise ValueError("Branding requires an explicit filesystem/source-oracle fixture")
        catalog = ViewCatalog(request)
        view, by_key, hidden = source["build_view"](
            catalog, "", request["show_published"], request["keep_sets"], seed_order=False)
        expected_calls = ["all_items"]
        if not request["show_published"]:
            expected_calls.append("published_keys")
            if request["keep_sets"]:
                expected_calls.append("published_set_names")
        if catalog.calls != expected_calls:
            raise ValueError(f"View oracle stub contract changed: {catalog.calls}")
        case["expected"] = {"view": view,
                            "by_key": {key: str(path) for key, path in by_key.items()},
                            "hidden": hidden}
    document = {"source": VIEW_SOURCE, "cases": cases}
    validate_cases(document, source=VIEW_SOURCE, count=3)
    return document
