"""Tests for what the curate panel builds and serves from the catalog.

The brand logo is never part of the catalog (it is injected only at publish
time by build_collection.py), but the panel should still show a preview card
for it -- without letting it affect the real included/excluded counts or be
sent to /api/save. These tests lock down that separation, together with the
view filtering, the similarity order and the concurrent-save window.

The build_view cases live in test_panel_view.py, the page-text assertions
in test_panel_page_grid.py and test_panel_page_actions.py, the POST guard in
test_panel_guard.py and the process/port behaviour in test_panel_server.py.
"""

from __future__ import annotations

import json
import sys
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer
from pathlib import Path
from unittest import mock
from urllib import error, request

from tests._panel_fixtures import ROOT, _make_png

sys.path.insert(0, str(ROOT))

from emojikit import panel as p
from emojikit.catalog import Catalog


class InertItemJson(unittest.TestCase):
    """A catalog label must never be able to escape the data block."""

    def test_script_close_is_escaped(self):
        out = p._json_for_script([{"label": "</script><script>alert(1)</script>"}])
        self.assertNotIn("</script>", out)
        self.assertNotIn("<", out)
        self.assertIn("\\u003c", out)

    def test_js_line_separators_are_escaped(self):
        out = p._json_for_script([{"label": "a b c"}])
        self.assertNotIn(" ", out)
        self.assertNotIn(" ", out)

    def test_roundtrips(self):
        items = [{"key": "s:1", "label": "ok <b>", "included": True}]
        self.assertEqual(json.loads(p._json_for_script(items)), items)


class _WatchedLock:
    """A lock that announces every acquisition *attempt*.

    That signal is the whole synchronisation point of the test below: the panel
    reads ``view`` either just before this fires (the defect) or just after it
    (the fix), so the test never has to wait on something that must not happen.
    """

    def __init__(self):
        self._lock = threading.Lock()
        self.attempted = threading.Event()

    def __enter__(self):
        self.attempted.set()
        self._lock.acquire()
        return self

    def __exit__(self, *exc):
        self._lock.release()


class _Tripwire(dict):
    """A view entry that suspends whatever is reading the view mid-``sort()``.

    ``list.sort()`` empties the list *before* calling the key function, so a
    key call that finds the list empty is running inside the sort -- exactly
    the window the concurrent save has to land in.
    """

    def __init__(self, item, view, entered, release):
        super().__init__(item)
        self._view, self._entered, self._release = view, entered, release

    def get(self, key, *default):
        if key == "isLogo" and not self._view:
            self._entered.set()
            self._release.wait(timeout=30)
        return super().get(key, *default)


class SaveDuringReorder(unittest.TestCase):
    """A save landing mid-``view.sort()`` must not wipe the user's curation.

    ``/api/order`` sorts ``view`` in place and CPython empties a list for the
    duration of ``list.sort()``. Reading ``view`` outside the lock therefore saw
    zero known keys, intersected the posted exclusions down to nothing, and
    ``set_inclusion(set())`` re-included every row -- while still answering
    ``{"ok": true}``. The panel debounces its order POST by 400 ms, so "drag,
    then hit Save" is the ordinary way to land in that window.

    The interleaving is pinned, not raced: the reorder is suspended inside
    ``sort()``, and the save is only released once it has reached the lock.
    """

    TOKEN = "test-token-value"

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        data = Path(self.tmp.name)
        self.db = data / "catalog.db"
        with Catalog(self.db) as cat:
            for i in range(4):
                img = data / "media" / "static" / f"i{i}.png"
                _make_png(img)
                cat.add(content_key=f"s:item{i:030d}", fmt="static", file_path=img,
                        emojis=["😀"], keywords=[f"item{i}"])
            self.view, by_key, _hidden = p.build_view(cat, "")

        self.sorting = threading.Event()
        self.release = threading.Event()
        self.view[0] = _Tripwire(self.view[0], self.view, self.sorting, self.release)

        self.lock = _WatchedLock()
        with mock.patch.object(threading, "Lock", lambda: self.lock):
            handler = p.make_handler(self.view, by_key, self.db, self.TOKEN)
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.release.set()          # never leave a suspended request behind
        self.httpd.shutdown()
        self.thread.join(timeout=10)
        self.assertFalse(self.thread.is_alive(), "panel server thread leaked")
        self.httpd.server_close()
        self.tmp.cleanup()

    def _post(self, path, body):
        req = request.Request(f"http://127.0.0.1:{self.port}{path}",
                              data=json.dumps(body).encode(), method="POST")
        req.add_header("Content-Type", "application/json")
        req.add_header("X-Panel-Token", self.TOKEN)
        try:
            with request.urlopen(req, timeout=30) as r:
                return r.status, json.loads(r.read() or b"{}")
        except error.HTTPError as e:
            return e.code, json.loads(e.read() or b"{}")

    def test_exclusions_survive_a_concurrent_reorder(self):
        keys = [v["key"] for v in self.view]
        excluded = keys[:2]
        results: dict = {}

        def run(name, path, body):
            results[name] = self._post(path, body)

        reorder = threading.Thread(target=run, args=(
            "order", "/api/order", {"order": list(reversed(keys))}), daemon=True)
        save = threading.Thread(target=run, args=(
            "save", "/api/save", {"excluded": excluded, "known": keys}), daemon=True)

        reorder.start()
        self.assertTrue(self.sorting.wait(timeout=30), "reorder never reached sort")
        # The reorder is now inside sort() with the view emptied. Anything the
        # save reads from here until release() is the corrupted snapshot.
        self.lock.attempted.clear()
        save.start()
        self.assertTrue(self.lock.attempted.wait(timeout=30),
                        "the save never reached the lock")
        self.release.set()
        for t in (reorder, save):
            t.join(timeout=30)
            self.assertFalse(t.is_alive(), "request thread leaked")

        self.assertEqual(results["order"][0], 200)
        self.assertEqual(results["save"][0], 200)
        self.assertEqual(results["save"][1]["excluded"], len(excluded),
                         "the save reported a count it did not persist")
        with Catalog(self.db) as cat:
            still_out = {it.content_key for it in cat.all_items() if not it.included}
        self.assertEqual(still_out, set(excluded),
                         "the de-selection was discarded by the concurrent sort")

    def test_the_page_is_not_served_from_an_emptied_view(self):
        """Serialising ``view`` unlocked during a sort rendered an empty grid."""
        keys = [v["key"] for v in self.view]
        page: dict = {}

        def fetch():
            with request.urlopen(
                    f"http://127.0.0.1:{self.port}/", timeout=30) as r:
                page["body"] = r.read().decode()

        reorder = threading.Thread(target=lambda: self._post(
            "/api/order", {"order": list(reversed(keys))}), daemon=True)
        reorder.start()
        self.assertTrue(self.sorting.wait(timeout=30), "reorder never reached sort")
        self.lock.attempted.clear()
        getter = threading.Thread(target=fetch, daemon=True)
        getter.start()
        # Fast on a correct handler (it takes the lock immediately); only a
        # handler that skipped the lock entirely ever waits this out.
        self.assertTrue(self.lock.attempted.wait(timeout=10),
                        "do_GET read the view without taking the lock")
        self.release.set()
        for t in (reorder, getter):
            t.join(timeout=30)
            self.assertFalse(t.is_alive(), "request thread leaked")

        for k in keys:
            self.assertIn(k, page["body"], "the grid was served without its items")


class CatalogUnavailable(unittest.TestCase):
    """A busy/unopenable catalog must produce an answer, not a dropped socket.

    build_collection.py reads the same database file, so sqlite3.Error is
    routine here. Uncaught it escaped do_POST and closed the connection with no
    HTTP response at all, leaving the panel with nothing to report.
    """

    TOKEN = "test-token-value"

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        data = Path(self.tmp.name)
        self.db = data / "catalog.db"
        with Catalog(self.db) as cat:
            img = data / "media" / "static" / "i0.png"
            _make_png(img)
            cat.add(content_key="s:item0", fmt="static", file_path=img)
            self.view, by_key, _hidden = p.build_view(cat, "")

        # Replace the database with a directory: sqlite3 then refuses to open
        # it, which is a real sqlite3.Error on the same code path a locked
        # database takes, without a five-second busy wait.
        for f in data.glob("catalog.db*"):
            f.unlink()
        self.db.mkdir()

        handler = p.make_handler(self.view, by_key, self.db, self.TOKEN)
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.httpd.shutdown()
        self.thread.join(timeout=10)
        self.assertFalse(self.thread.is_alive(), "panel server thread leaked")
        self.httpd.server_close()
        self.tmp.cleanup()

    def _post(self, path, body):
        req = request.Request(f"http://127.0.0.1:{self.port}{path}",
                              data=json.dumps(body).encode(), method="POST")
        req.add_header("Content-Type", "application/json")
        req.add_header("X-Panel-Token", self.TOKEN)
        try:
            with request.urlopen(req, timeout=10) as r:
                return r.status, json.loads(r.read() or b"{}")
        except error.HTTPError as e:
            return e.code, json.loads(e.read() or b"{}")

    def test_save_answers_503(self):
        code, body = self._post("/api/save", {"excluded": ["s:item0"],
                                              "known": ["s:item0"]})
        self.assertEqual(code, 503)
        self.assertIn("catalog unavailable", body["error"])

    def test_order_answers_503(self):
        code, body = self._post("/api/order", {"order": ["s:item0"]})
        self.assertEqual(code, 503)
        self.assertIn("catalog unavailable", body["error"])

    def test_a_page_served_from_a_busy_catalog_says_so(self):
        with request.urlopen(f"http://127.0.0.1:{self.port}/", timeout=10) as r:
            page = r.read().decode("utf-8")
        self.assertRegex(page, r'const STALE = "\d\d:\d\d";',
                         "a view the refresh could not update must be marked as old")


class TheMediaMapIsNeverEmpty(unittest.TestCase):
    def test_a_reader_never_misses_a_key_both_maps_hold(self):
        from emojikit.panel_save import replace_map
        shared = {f"k{i}": i for i in range(200)}
        fresh = {f"k{i}": -i for i in range(100, 300)}
        missed, done = [], threading.Event()

        def read():
            while not done.is_set():
                for k in ("k100", "k150", "k199"):
                    if k not in shared:
                        missed.append(k)

        reader = threading.Thread(target=read)
        reader.start()
        try:
            for _ in range(1000):
                replace_map(shared, fresh)
                replace_map(shared, {f"k{i}": i for i in range(200)})
        finally:
            done.set()
            reader.join(10)
        self.assertEqual(missed, [])


class SavingFromAFilteredGridKeepsHiddenChoices(unittest.TestCase):
    """A view that hides things cannot speak for what it hides.

    `set_inclusion` re-includes every key it is NOT given, so a save posted
    from a grid that hides finished packs would silently re-include every
    hidden item that had been deselected. The panel already carries a comment
    about an earlier variant of exactly this; the filter reintroduced it.
    """

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.data = Path(self.tmp.name)
        self.db = self.data / "catalog.db"
        with Catalog(self.db) as cat:
            for i in range(4):
                img = self.data / "media" / "static" / f"i{i}.png"
                _make_png(img)
                cat.add(content_key=f"s:item{i:030d}", fmt="static", file_path=img,
                        emojis=["😀"], keywords=[f"item{i}"])
            for i in (0, 1):
                cat.mark_uploaded(f"s:item{i:030d}", f"cid{i}",
                                  base="pk", set_name="pk1_by_bot")
            # the owner deselected one of the FINISHED pack's emoji
            cat.set_inclusion({f"s:item{0:030d}"})
        (self.data / "publish_pk.json").write_text(json.dumps({
            "sets": [{"name": "pk1_by_bot", "index": 1, "live": p.PER_SET,
                      "logo": True}]}), encoding="utf-8")

    def tearDown(self):
        self.tmp.cleanup()

    def test_a_hidden_exclusion_survives_a_save_from_the_filtered_grid(self):
        with Catalog(self.db) as cat:
            view, _bk, hidden = p.build_view(cat, "", False)
        self.assertEqual(hidden, 2, "the published pack is out of the grid")
        visible = {v["key"] for v in view if not v.get("isLogo")}
        self.assertNotIn(f"s:item{0:030d}", visible)

        # What the handler does: intersect the request with what is shown, then
        # carry every hidden exclusion through.
        raw: set[str] = set()                       # nothing deselected on screen
        with Catalog(self.db) as cat:
            hidden_excluded = {it.content_key for it in cat.all_items()
                               if not it.included and it.content_key not in visible}
            cat.set_inclusion((raw & visible) | hidden_excluded)
            still = {it.content_key for it in cat.all_items() if not it.included}
        self.assertEqual(still, {f"s:item{0:030d}"},
                         "the hidden de-selection must not be undone")


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_panel -v")
