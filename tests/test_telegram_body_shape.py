"""Telegram._call: a JSON body that is not an object is a transport failure.

`_call` called `payload.get(...)` on whatever JSON came back. A proxy answering
with `[]` or `"ok"` raised AttributeError straight out of the client, past the
retry loop and past the applied-check that decides whether a non-idempotent
add already landed.
"""

from __future__ import annotations

import unittest
from unittest import mock


from emojikit import telegram_api as tg_api


class _Reply:
    status_code = 200

    def __init__(self, body):
        self._body = body

    def json(self):
        return self._body


class ANonObjectBodyGoesThroughTheRetryPath(unittest.TestCase):

    def _tg(self, body):
        tg = tg_api.Telegram("unit-test-token")
        tg.s = mock.Mock()
        tg.s.post.return_value = _Reply(body)
        return tg

    def test_a_list_body_is_retried_then_reported_not_an_attribute_error(self):
        for body in ([], "ok", 42):
            with self.subTest(body=body):
                tg = self._tg(body)
                with mock.patch.object(tg_api.time, "sleep", lambda s: None), \
                        mock.patch("builtins.print"):
                    with self.assertRaises(RuntimeError) as caught:
                        tg._call("getMe", retries=2)
                self.assertNotIsInstance(caught.exception, AttributeError)
                self.assertEqual(tg.s.post.call_count, 2)

    def test_an_add_with_a_body_that_is_not_an_object_asks_the_applied_check(self):
        tg = self._tg([])
        with mock.patch.object(tg_api.time, "sleep", lambda s: None), \
                mock.patch("builtins.print"):
            result = tg._call("addStickerToSet", data={}, retries=2,
                              applied_check=lambda: True)
        self.assertEqual(result, {"verified_applied": True})


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_telegram_body_shape -v")
