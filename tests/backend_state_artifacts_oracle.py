"""Plan keys and generic publisher remapping retain their distinct original contracts."""
import copy
import json
from pathlib import Path
from tests.backend_source_oracle import source_namespace, validate_cases
SOURCE = "d173116:state_artifacts.rewrite"
FIXTURE = Path(__file__).parent / "fixtures/backend/state-artifacts.json"


def fixtures():
    source = source_namespace("state_artifacts.py", ("_plan_refs", "remap_plan"), {"copy": copy})
    generic = source_namespace("collection_migrate.py", ("_remap",), {})
    cases = [
        {"name": "pack_plan.json", "document": {"version": 1, "moves": [{"key": "v:old", "label": "v:old"}],
            "held": [{"key": "v:old"}], "targets": [["v:old", 6]], "excluded": [], "known": ["v:old"]}},
        {"name": "publish_fixture.json", "document": {"base": "fixture", "sets": [{"keys": ["v:old", "s:keep"]}],
            "skipped": ["v:old"], "in_flight": {"key": "v:old", "title": "v:old"}}},
        {"name": "publish_plan_fixture.json", "document": {"video": ["v:old"], "static": ["s:keep"]}},
    ]
    for case in cases:
        case["mapping"] = {"v:old": "v:new"}
        function = source["remap_plan"] if case["name"] == "pack_plan.json" else generic["_remap"]
        case["expected"] = function(case["document"], case["mapping"])
    value = {"source": SOURCE, "cases": cases}
    validate_cases(value, source=SOURCE, count=3)
    return value


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    value = fixtures()
    if args.write:
        FIXTURE.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != value:
        raise SystemExit("Fixture drift: state-artifacts.json")
