"""One renderer for every "emoji in this pack" table (emojikit/pack_rows.py).

Four writers each carried a copy: one cut the keywords to two, one did not
escape `|`, one had no test at all. These feed a keyword that is hostile to a
Markdown table -- a `|` and a newline -- through all four, and pin each file's
column headers so sharing the renderer changed none of them.
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path


from emojikit import collection_notify, pack_archive, pack_manifest, pack_rows
from emojikit.catalog import Catalog
from coins import write_manifests
from tests._media_fixtures import make_png

HOSTILE = "left|right\nnext line"
ESCAPED = "left\\|right next line"


class TheSharedRenderer(unittest.TestCase):
    def test_pipes_and_newlines_are_escaped_and_empty_is_a_dash(self):
        table = pack_rows.markdown_table([{"a": HOSTILE, "b": ""}],
                                         [("a", "A"), ("b", "B")])
        self.assertEqual(table.splitlines()[2], f"| {ESCAPED} | — |")

    def test_every_keyword_is_kept(self):
        self.assertEqual(pack_rows.keyword_cell(["a", "", "b", "c"]), "a, b, c")


class EveryWriterUsesIt(unittest.TestCase):

    def assert_row(self, text: str, header: str):
        self.assertIn(header, text, "a writer's column headers changed")
        self.assertIn(ESCAPED, text)
        self.assertNotIn("left|right", text, "a raw | breaks the table")

    def test_the_publishers_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = Path(tmp)
            img = make_png(data / "media" / "a.png")
            with Catalog(data / "catalog.db") as cat:
                cat.add(content_key="s:a", fmt="static", file_path=img,
                        emojis=["\U0001F600"], keywords=["one", "two", HOSTILE])
                collection_notify.write_manifest(
                    data, cat, {"name": "pk1_by_bot", "title": "P", "fmt": "static",
                                "keys": ["s:a"]}, "pk")
            text = (data / "manifests" / "pk1_by_bot.md").read_text(encoding="utf-8")
        self.assert_row(text, "| # | Name | Emoji ID |")
        self.assertIn("one, two, left", text, "all keywords, not the first two")

    def test_the_archive_manifest(self):
        text = pack_archive.render_manifest_md(
            {"name": "set1", "title": "One", "fmt": "mixed"},
            [{"content_key": "s:a", "premium_id": "111"}],
            {"s:a": {"keywords": [HOSTILE]}})
        self.assert_row(text, "| # | Name | Emoji ID |")

    def test_the_roster(self):
        class _TG:
            def get_sticker_set(self, _name):
                return {"stickers": [{"custom_emoji_id": "111", "emoji": "✅"}]}

        doc = pack_manifest.build_pack(_TG(), {"name": "s", "index": 1, "logo": False},
                                       "general", {}, set())
        doc["emoji"][0]["name"] = HOSTILE
        self.assert_row(pack_manifest.render_markdown(doc),
                        "| # | slot | custom_emoji_id | ours before | glyph | format | name | was |")

    def test_the_coin_manifest(self):
        text = write_manifests.render_pack_md(
            {"name": "coin1_by_bot", "title": "C 1"},
            [{"custom_emoji_id": "111"}], {"111": [HOSTILE]})
        self.assert_row(text, "| # | Ticker(s) | Emoji ID |")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_pack_rows -v")
