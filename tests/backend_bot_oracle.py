"""Bot rendering expected payloads generated only from pinned pre-port pure source."""
import html
import json
import logging
from pathlib import Path
import re

from tests.backend_source_oracle import source_namespace, validate_cases

SOURCE = "d173116:emoji_bot.pure_payloads"
FIXTURE = Path(__file__).parent / "fixtures/backend/bot.json"


def fixtures():
    source = source_namespace("emoji_bot.py", ("parse_id_list", "extract_custom_emoji_ids",
        "_fallback_char", "_emoji_span", "_batch_ids", "_render_message", "_copy_text",
        "_chunk_for_copy", "_copy_keyboard", "build_payloads"), {
        "html": html, "log": logging.getLogger("bot-oracle"), "DEFAULT_FALLBACK": "⭐",
        "PER_ID_COST": 110, "COPY_MAX": 256, "MSG_MAX": 3500,
        "_ID_LIST_RE": re.compile(r"^\d{15,25}(?:[\s,;]+\d{15,25})*$"),
        "_ID_SEP_RE": re.compile(r"[\s,;]+"), "_CUSTOM_EMOJI_ID": re.compile(r"\d{1,25}")})
    cases = [{"name": "empty-payload", "operation": "payloads", "ids": [], "labels": {}, "rich": True},
        {"name": "html-escaped-rich", "operation": "payloads", "ids": ["111111111111111", "222222222222222"],
         "labels": {"111111111111111": "<&'\""}, "rich": True},
        {"name": "plain-long-copy-batches", "operation": "payloads",
         "ids": [str(10**24 + i) for i in range(31)], "labels": {}, "rich": False},
        {"name": "typed-order-dedup", "operation": "typed", "text": " 111111111111111,222222222222222;111111111111111 "},
        {"name": "prose-not-lookup", "operation": "typed", "text": "id 111111111111111"},
        {"name": "quoted-entity-order", "operation": "extract", "message": {
            "entities": [{"type": "custom_emoji", "custom_emoji_id": "111111111"}],
            "caption_entities": [{"type": "custom_emoji", "custom_emoji_id": "<bad>"}],
            "quote": {"entities": [{"type": "custom_emoji", "custom_emoji_id": "222222222"}]},
            "external_reply": {"quote": {"entities": [{"type": "custom_emoji", "custom_emoji_id": "111111111"},
                                                       {"type": "custom_emoji", "custom_emoji_id": "333333333"}]}}}}]
    for case in cases:
        if case["operation"] == "payloads":
            case["expected"] = [list(row) for row in source["build_payloads"](case["ids"], case["labels"], case["rich"])]
        elif case["operation"] == "typed":
            case["expected"] = source["parse_id_list"](case["text"])
        else:
            case["expected"] = source["extract_custom_emoji_ids"](case["message"])
    document = {"source": SOURCE, "cases": cases}
    validate_cases(document, source=SOURCE, count=6)
    return document


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    value = fixtures()
    if args.write:
        FIXTURE.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    elif json.loads(FIXTURE.read_text(encoding="utf-8")) != value:
        raise SystemExit("Fixture drift: bot.json")
