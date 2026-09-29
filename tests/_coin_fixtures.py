"""Shared fakes for the coin-provider test modules.

Leading underscore is load-bearing: ``discover -p "test_*.py"`` must not
collect this as a suite. Nothing here owns a ``test_*`` method, so
importing it into several modules cannot inflate the count.
"""

from __future__ import annotations

import io
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

import requests  # noqa: E402
from PIL import Image  # noqa: E402

from emojikit import telegram_api as tg_api  # noqa: E402
from emojikit.packstate import (LockBusy, canonical_map_lock, exclusive_lock,  # noqa: E402
                                write_json_atomic)

SET = "cryptoemoji1_by_bot"


def _png_bytes(img: Image.Image) -> bytes:
    buf = io.BytesIO()
    img.save(buf, "PNG")
    return buf.getvalue()


def _gradient(reverse: bool = False) -> Image.Image:
    """Opaque left-to-right (or right-to-left) grey ramp.

    Two ramps of opposite direction are perceptually as far apart as a dHash can
    report, which is what makes the content match in the tests unambiguous.
    """
    img = Image.new("RGBA", (100, 100))
    px = img.load()
    for x in range(100):
        v = (99 - x) * 2 if reverse else x * 2
        for y in range(100):
            px[x, y] = (v, v, v, 255)
    return img


class FakeTelegram:
    """Deterministic stand-in for build_pack.Telegram."""

    def __init__(self, existing: int = 2):
        self.sets: dict[str, list[dict]] = {SET: []}
        self.blobs: dict[str, bytes] = {}
        self.adds: list[tuple[str, str, int | None]] = []
        self.creates: list[str] = []
        self.fail_add: Exception | None = None
        self.after_add = None          # hook: simulates a concurrent writer
        self.unreadable: set[str] = set()   # sets Telegram will not talk about
        self._n = 0
        for _ in range(existing):
            self.append(SET, _png_bytes(_gradient(reverse=True)))

    # ----- fake wire ------------------------------------------------------ #
    def append(self, name: str, data: bytes) -> dict:
        self._n += 1
        st = {"custom_emoji_id": f"c{self._n}", "file_id": f"f{self._n}",
              "file_unique_id": f"u{self._n}"}
        self.blobs[st["file_id"]] = data
        self.sets.setdefault(name, []).append(st)
        return st

    # ----- Telegram surface used by publish_logos ------------------------- #
    def get_me(self) -> dict:
        return {"username": "bot"}

    def get_sticker_set(self, name: str) -> dict:
        if name in self.unreadable:
            raise RuntimeError("getStickerSet failed after 5 attempts")
        if name not in self.sets:
            raise RuntimeError("getStickerSet failed: STICKERSET_INVALID")
        return {"stickers": [dict(s) for s in self.sets[name]]}

    def probe_set_state(self, name: str):
        if name in self.unreadable:
            return tg_api.SetState.UNKNOWN, None
        if name not in self.sets:
            return tg_api.SetState.MISSING, None
        return tg_api.SetState.EXISTS, {"stickers": [dict(s) for s in self.sets[name]]}

    def add_sticker(self, user_id, name, png, emoji, keywords, *,
                    expected_before=None):
        self.adds.append((name, Path(png).stem, expected_before))
        if self.fail_add:
            raise self.fail_add
        self.append(name, Path(png).read_bytes())
        if self.after_add:
            self.after_add(self, name)

    def create_set(self, user_id, name, title, png, emoji, keywords):
        self.creates.append(name)
        self.sets.setdefault(name, [])
        self.append(name, Path(png).read_bytes())

    def download_file(self, file_id: str, dest: Path) -> Path:
        Path(dest).write_bytes(self.blobs[file_id])
        return dest


# --------------------------------------------------------------------------- #
# The live-set fakes remap_ids and check_all_packs read from. Named apart from
# FakeTelegram above, which stands in for the providers' publishing surface.
# --------------------------------------------------------------------------- #
def _png(color, size=100) -> bytes:
    """A PNG with a solid block of ``color`` on transparent background."""
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    im.paste(color, (20, 20, 80, 80))
    buf = io.BytesIO()
    im.save(buf, "PNG")
    return buf.getvalue()


def _blank_png(size=100) -> bytes:
    buf = io.BytesIO()
    Image.new("RGBA", (size, size), (0, 0, 0, 0)).save(buf, "PNG")
    return buf.getvalue()


class FakeResponse:
    def __init__(self, content: bytes, status: int = 200):
        self.content = content
        self.status_code = status

    def raise_for_status(self):
        if self.status_code >= 400:
            # The real message embeds the URL (and therefore the bot token).
            raise requests.HTTPError(f"{self.status_code} Server Error for TOKEN123")


class FakeSession:
    """Serves file bodies by file_path; ``fail`` names always return 500."""

    def __init__(self, blobs: dict[str, bytes], fail: set[str] = frozenset()):
        self.blobs = blobs
        self.fail = set(fail)
        self.fetched: list[str] = []

    def get(self, url, timeout=None):
        fid = url.rsplit("/", 1)[1].removesuffix(".png")
        self.fetched.append(fid)
        if fid in self.fail:
            return FakeResponse(b"{'ok':false}", status=500)
        return FakeResponse(self.blobs[fid])


class LiveSetsTelegram:
    """Live packs: {set name: [{custom_emoji_id, file_unique_id, file_id}, ...]}."""

    def __init__(self, sets: dict[str, list[dict]], on_read=None):
        self.sets = sets
        # Runs inside the live read -- the window a concurrent provider used to
        # slip through between remap's read of the packs and its write.
        self.on_read = on_read

    def get_me(self):
        return {"username": "coinbot"}

    def get_sticker_set(self, name):
        if self.on_read:
            self.on_read()
        return {"stickers": [dict(s) for s in self.sets[name]]}

    # Serves file bytes the way Telegram.download_bytes does: through a
    # FakeSession the test hands in, an error status raising after "retries".
    session = None

    def call(self, method, *, data=None, files=None, retries=5,
             applied_check=None):
        assert method == "getFile", method
        return {"file_path": f"stickers/{data['file_id']}.png"}

    def download_bytes(self, file_id, retries=5):
        path = self.call("getFile", data={"file_id": file_id})["file_path"]
        r = self.session.get(path, timeout=60)
        try:
            r.raise_for_status()
        except requests.HTTPError as exc:
            # The real client gives up with a message that names no URL.
            raise RuntimeError(f"download failed for file_id {file_id}") from exc
        return r.content

    def safe(self, exc):
        return str(exc).replace("TOKEN123", "[REDACTED]")


class ThrottledTelegram(LiveSetsTelegram):
    """Telegram that rate-limits the set LISTING, or dies during one.

    ``fail_times`` is how many listings of a set answer 429 before it succeeds;
    ``kill_on`` names a set whose listing kills the process outright, which is
    what an interrupted run looks like from inside main().
    """

    def __init__(self, sets: dict[str, list[dict]], fail_times: dict | None = None,
                 kill_on: str | None = None):
        super().__init__(sets)
        self.fail_times = dict(fail_times or {})
        self.kill_on = kill_on
        self.listings = 0

    def get_sticker_set(self, name):
        self.listings += 1
        if name == self.kill_on:
            raise KeyboardInterrupt("killed mid-run")
        left = self.fail_times.get(name, 0)
        if left:
            self.fail_times[name] = left - 1
            # The real message embeds the API URL, and therefore the bot token.
            raise RuntimeError("429 Too Many Requests for TOKEN123")
        return super().get_sticker_set(name)


def _sticker(cid: str, fuid: str | None = None) -> dict:
    return {"custom_emoji_id": cid, "file_unique_id": fuid or f"fu-{cid}",
            "file_id": cid}


def _provider_top_up(pack_lock: Path, out: Path, ticker: str, cid: str):
    """A provider adding one sticker and its map entry, as fetch_paprika does.

    Same locks in the same documented order (pack family first, canonical map
    second). Returns the LockBusy it hit, or None when it got all the way
    through -- which is the whole question this file's race tests ask.
    """
    try:
        with exclusive_lock(pack_lock), canonical_map_lock():
            mapping = json.loads(out.read_text("utf-8")) if out.is_file() else {}
            mapping[ticker] = cid
            write_json_atomic(out, mapping)
    except LockBusy as exc:
        return exc
    return None
