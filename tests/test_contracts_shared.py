"""The Python half of the Python <-> Worker contracts, read from shared fixtures.

`announce_via_worker` POSTs to the Worker, and every other Python test replaces
it; the Worker only validated hand-written bodies. So either side could change
the publish body and stay green while announcements were lost (the callers
swallow a failed announce by design). The same goes for the custom-emoji id
syntax, which the bot and the Worker each hold a copy of.

`tests/fixtures/contracts/` is read by `worker/test/contracts.test.ts` too:
changing either side now breaks both suites together.
"""

from __future__ import annotations

import json
import os
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parent.parent

from emojikit import announce  # noqa: E402
from tests.reference import emoji_bot  # noqa: E402

FIXTURES = ROOT / "tests" / "fixtures" / "contracts"


def _fixture(name: str):
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


class ThePublishBodyIsWhatTheWorkerValidates(unittest.TestCase):

    def test_announce_via_worker_sends_the_fixture_body(self):
        want = _fixture("publish_request.json")
        env = {"WORKER_PUBLISH_URL": "https://worker.invalid/publish",
               "WORKER_PUBLISH_SECRET": "publish-bearer"}
        reply = mock.Mock(status_code=200, text="ok")
        with mock.patch.dict(os.environ, env), \
                mock.patch.object(announce.requests, "post",
                                  return_value=reply) as post:
            announce.announce_via_worker(want["packs"], note=want["note"],
                                         bot=want["bot"], style=want["style"])
        (url,), kw = post.call_args
        self.assertEqual(url, env["WORKER_PUBLISH_URL"])
        self.assertEqual(kw["json"], want,
                         "the body changed: update the shared fixture and the "
                         "Worker's validator together")
        self.assertTrue(kw["headers"]["Authorization"].startswith("Bearer "))


class TheIdSyntaxIsSharedWithTheWorker(unittest.TestCase):

    def test_every_case(self):
        for case in _fixture("emoji_messages.json"):
            with self.subTest(case["name"]):
                msg = case["message"]
                self.assertEqual(emoji_bot.extract_custom_emoji_ids(msg), case["ids"])
                self.assertEqual(emoji_bot.parse_id_list(msg.get("text", "")),
                                 case["typed"])


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_contracts_shared -v")
