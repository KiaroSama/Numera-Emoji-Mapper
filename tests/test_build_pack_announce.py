"""build_pack announces finished packs as the bot whose token it publishes with.

It always said "general". With the coin bot's token as the default, coin packs
were announced by the wrong bot once a Worker was deployed, and the Worker and
direct routes disagreed about who had published them.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


from emojikit import build_pack as bp
from emojikit import telegram_api as tg_api
from tests._pack_fixtures import _png


def _one_set_telegram() -> mock.Mock:
    """A set that exists once it has been created, holding what was sent."""
    tg = mock.Mock()
    tg.get_me.return_value = {"username": "bot"}
    created: list[bool] = []

    def probe(_name):
        if not created:
            return tg_api.SetState.MISSING, None
        return tg_api.SetState.EXISTS, {"stickers": [{"file_unique_id": "f0"}]}

    tg.create_set.side_effect = lambda *_a, **_k: created.append(True)
    tg.probe_set_state.side_effect = probe
    tg.probe_sticker_set.side_effect = lambda n: (True, probe(n)[1])
    tg._sticker_matches.return_value = True
    return tg


class TheAnnouncerMatchesTheToken(unittest.TestCase):

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name)
        _png(self.dir / "src" / "a.png")

    def _bot_announced(self, token_env: str) -> list[str]:
        seen: list[str] = []

        def capture(tg, owner, packs, *, bot, **kw):
            seen.append(bot)
            return "owner"

        argv = ["build_pack.py", "--base", "t", "--title", "T", "--user-id", "1",
                "--source-dir", str(self.dir / "src"), "--token-env", token_env,
                "--state", str(self.dir / f"state_{token_env}.json")]
        with mock.patch.object(sys, "argv", argv), \
             mock.patch.object(bp, "setup_logging", lambda *a, **k: None), \
             mock.patch.dict("os.environ", {token_env: "x"}, clear=False), \
             mock.patch.object(bp, "Telegram", return_value=_one_set_telegram()), \
             mock.patch.object(bp, "announce_packs", capture), \
             mock.patch.object(bp.time, "sleep", lambda s: None):
            self.assertEqual(bp.main(), bp.EXIT_OK)
        return seen

    def test_the_coin_token_announces_as_the_coin_bot(self):
        self.assertEqual(self._bot_announced("TELEGRAM_BOT_TOKEN"), ["coin"])

    def test_any_other_token_announces_as_the_general_bot(self):
        self.assertEqual(self._bot_announced("GENERAL_BOT_TOKEN"), ["general"])


if __name__ == "__main__":
    # A direct run skips tests/__init__.py, the credential scrub and socket
    # block that exist because a test once changed a live pack.
    raise SystemExit("Run this suite as: python -m unittest tests.test_build_pack_announce -v")
