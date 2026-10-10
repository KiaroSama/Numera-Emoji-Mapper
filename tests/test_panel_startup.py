"""The panel listens and opens the browser without waiting on Telegram.

The bot name only decides whether the brand-logo preview card shows, yet it was
fetched before the server even bound its port: with Telegram unreachable the
whole start-up waited out five retries.
"""
from __future__ import annotations

import socket
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

from tests._panel_fixtures import _make_png

from tests.reference import panel
from tests.reference.catalog import Catalog


def _free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class StartUpDoesNotWaitOnTelegram(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.data = Path(tmp.name)
        img = self.data / "media" / "static" / "i0.png"
        _make_png(img)
        with Catalog(self.data / "catalog.db") as cat:
            cat.add(content_key="s:item0", fmt="static", file_path=img)

    def _main(self, port):
        argv = ["panel", "--data-dir", str(self.data), "--port", str(port), "--no-open"]
        with mock.patch("sys.argv", argv), mock.patch.object(panel, "setup_logging"):
            return panel.main()

    def test_a_hanging_bot_lookup_does_not_hold_the_start(self):
        never = threading.Event()
        self.addCleanup(never.set)

        def hang(retries=5):
            never.wait(10)
            return ""

        with mock.patch.object(panel, "_detect_bot_username", hang), \
                mock.patch.object(panel.ThreadingHTTPServer, "serve_forever",
                                  side_effect=KeyboardInterrupt), \
                mock.patch.object(panel.webbrowser, "open"):
            started = time.monotonic()
            self._main(_free_port())
            self.assertLess(time.monotonic() - started, 2.0)

    def test_a_running_panel_on_the_port_is_reopened_not_rebuilt(self):
        with socket.socket() as busy:
            busy.bind(("127.0.0.1", 0))
            busy.listen()
            with mock.patch.object(panel, "reopen_existing", return_value=0) as reopen:
                self.assertEqual(self._main(busy.getsockname()[1]), 0)
        reopen.assert_called_once()


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_panel_startup -v")
