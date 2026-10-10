"""Original live-pack shaping with prefetched API evidence and a fixed source clock."""
from datetime import datetime, timezone
import json
from pathlib import Path
from types import SimpleNamespace
from tests.backend_source_oracle import source_namespace, validate_cases

SOURCE = "d173116:pack_manifest.build_pack"
FIXTURE = Path(__file__).parent / "fixtures/backend/roster-pack.json"


def fixtures():
    class Clock:
        @staticmethod
        def now(tz=None):
            return datetime(2026, 10, 7, tzinfo=tz)
    media = SimpleNamespace(telegram_sticker_format=lambda st:
        "animated" if st.get("is_animated") else "video" if st.get("is_video") else "static")
    source = source_namespace("pack_manifest.py", ("_row", "build_pack"), {
        "media": media, "datetime": Clock, "timezone": timezone})
    cases = [
        {"name": "first-catalog-item-is-not-logo", "family": "general",
         "record": {"name": "fixture_by_YourEmojiBot", "title": "Fixture", "index": 1},
         "pack": {"title": "not-authoritative", "stickers": [{"custom_emoji_id": "111111111", "emoji": "😀"}]},
         "provenance": {"111111111": {"content_key": "s:fixture", "name": "fixture", "source_emoji_ids": ["222222222"]}},
         "prior": {"ck:s:fixture": {"id": "333333333", "history": ["444444444"]}}},
        {"name": "logo-history-is-per-set", "family": "general",
         "record": {"name": "fixture2_by_YourEmojiBot", "title": None, "index": 2},
         "pack": {"stickers": [{"custom_emoji_id": "111111111"}, {"custom_emoji_id": "222222222", "is_animated": True}]},
         "provenance": {}, "prior": {"logo:fixture2_by_YourEmojiBot": {"id": "333333333", "history": []}}},
        {"name": "unbranded-first-sticker", "family": "general",
         "record": {"name": "fixture_by_YourEmojiBot", "title": "Fixture", "index": 1, "logo": False},
         "pack": {"stickers": [{"custom_emoji_id": "111111111"}]}, "provenance": {}, "prior": {}},
        {"name": "shared-coin-logo-retains-all-tickers", "family": "coins",
         "record": {"name": "fixture_by_YourEmojiBot", "title": "Coins", "index": 1},
         "pack": {"stickers": [{"custom_emoji_id": "111111111"}, {"custom_emoji_id": "222222222", "is_video": True}]},
         "provenance": {"111111111": ["aaa", "bbb"]},
         "prior": {"coin:aaa,bbb": {"id": "333333333", "history": []}}},
    ]
    for case in cases:
        tg = SimpleNamespace(get_sticker_set=lambda name, pack=case["pack"]: pack)
        case["expected"] = source["build_pack"](tg, case["record"], case["family"],
            case["provenance"], set(), case["prior"])
    table = source_namespace("pack_rows.py", ("_cell", "markdown_table"), {"EMPTY": "—"})
    renderer = source_namespace("pack_manifest.py", ("_zero_note", "render_markdown"), {"markdown_table": table["markdown_table"]})
    for case in cases:
        case["markdown"] = renderer["render_markdown"](case["expected"])
    value = {"source": SOURCE, "cases": cases}
    validate_cases(value, source=SOURCE, count=4)
    index_renderer = source_namespace("pack_manifest.py", ("render_index_markdown",), {})
    doc = cases[0]["expected"]
    packs = [{k: doc[k] for k in ("set_name", "title", "family", "pack_index", "link", "count")}]
    current = {e["custom_emoji_id"]: {"set": doc["set_name"], "slot": e["slot"], "index": e["index"]} for e in doc["emoji"]}
    foreign = {src: e["custom_emoji_id"] for e in doc["emoji"] for src in e["source_emoji_ids"]}
    retired = {src: e["custom_emoji_id"] for e in doc["emoji"] for src in e["previous_custom_emoji_ids"]}
    old = {"packs": [{"set_name": "coins_by_YourEmojiBot", "title": "Coins", "family": "coins", "pack_index": 1,
                      "link": "https://t.me/addemoji/coins_by_YourEmojiBot", "count": 1}],
           "by_current_id": {"999999999": {"set": "coins_by_YourEmojiBot", "slot": 1, "index": 0}},
           "by_source_id": {"888888888": "999999999"}, "by_previous_id": {"777777777": "999999999"},
           "inputs": {"catalog.db": [1, 1], "publish_fixture.json": [2, 2], "rebuild_dedup_state.json": [3, 3], "ticker_to_id.json": [4, 4]}}
    now = {k: [9, 9] for k in old["inputs"]}
    class OldIndex:
        def __truediv__(self, name):
            return self
        def read_text(self, encoding):
            return json.dumps(old)
    source = source_namespace("pack_manifest.py", ("_keep_other_family",), {
        "json": json, "OUT_DIR": OldIndex(), "_family_inputs": lambda family: {"catalog.db", "publish_fixture.json"}})
    inputs = source["_keep_other_family"]("general", packs, current, foreign, retired, now)
    expected = {"captured_utc": "2026-10-07 00:00:00 UTC", "pack_count": len(packs),
                "emoji_count": sum(p["count"] for p in packs), "packs": packs, "by_current_id": current,
                "by_source_id": foreign, "by_previous_id": retired, "id_changes": len(retired), "inputs": inputs}
    value["index_case"] = {"documents": [doc], "family": "general", "old": old, "inputs": now,
                           "expected": expected, "markdown": index_renderer["render_index_markdown"](expected)}
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
        raise SystemExit("Fixture drift: roster-pack.json")
