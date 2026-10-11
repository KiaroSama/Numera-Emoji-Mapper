"""Compatibility exports for retained coin tools; all HTTP/retry/upload proofs are native."""
from __future__ import annotations

import json
import logging
import os
from enum import Enum
from pathlib import Path
import tempfile

from emojikit.errors import FloodWaitTooLong

log = logging.getLogger("build_pack")
COIN_DEFAULT_EMOJI = "\U0001FA99"
MAX_PER_SET = PER_SET = 200
NAME_LOCK_TIMEOUT = 300.0
_MIME = {".png": "image/png", ".webp": "image/webp"}
_MIME_BY_EXT = {**_MIME, ".gif": "image/gif", ".tgs": "application/gzip", ".webm": "video/webm"}


def api_base() -> str:
    # Resolve after each caller's load_env, never at import time.
    base = os.environ.get("TELEGRAM_API_BASE")
    if base is None and os.environ.get("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1":
        base = os.environ.get("NUMERA_TEST_API_BASE")
    return (base or "https://api.telegram.org").rstrip("/")


class SetState(Enum):
    EXISTS = "exists"
    MISSING = "missing"
    UNKNOWN = "unknown"


class LiveStateUnknown(RuntimeError):
    """Live state cannot be determined; never infer an empty set."""


class BotApiError(RuntimeError):
    """A definite rejection, not an unanswered mutation."""


class AmbiguousUploadError(RuntimeError):
    """A mutation may be live and must be reconciled before another attempt."""


def _result(raw: str):
    envelope = json.loads(raw)
    if envelope["ok"]:
        return envelope["result"]
    kind, message = envelope["kind"], envelope["error"]
    if kind == "flood":
        raise FloodWaitTooLong(envelope["method"], envelope["seconds"])
    error = {"rejected": BotApiError, "ambiguous": AmbiguousUploadError,
             "invalid": ValueError}.get(kind, RuntimeError)
    raise error(message)


def _codec_compare(data: bytes, source: str, fmt: str) -> bool | None:
    # Only codec comparison runs in Python. Native owns download and attribution.
    try:
        from emojikit import identity
        root = Path(__file__).resolve().parents[1] / "logs" / "codec-verify"
        root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="sticker-", dir=root) as folder:
            path = Path(folder) / "sticker.dl"
            path.write_bytes(data)
            return identity.same_image(path, Path(source), fmt)
    except Exception:  # noqa: BLE001 - original codec probe failure remains unknown
        return None


class Telegram:
    def __init__(self, token: str) -> None:
        self.token = token
        self.max_flood_wait: float | None = None

    def _native(self):
        from emojikit import _native
        return _native.NativeTelegram(self.token, api_base())

    def _safe(self, exc) -> str:
        from emojikit.logsetup import redact
        return redact(str(exc).replace(self.token, "[REDACTED]"))

    safe = _safe

    def _call(self, method: str, *, data=None, files=None, retries: int = 5, applied_check=None):
        fields = {}
        for key, value in (data or {}).items():
            if value is not None:
                fields[str(key)] = str(value)
        uploads = []
        for field, spec in (files or {}).items():
            filename, content, *mime = spec
            if not isinstance(content, bytes):
                raise ValueError("native uploads require immutable bytes")
            uploads.append((field, filename, mime[0] if mime else "application/octet-stream", content))
        limit = None if self.max_flood_wait is None else max(0, int(self.max_flood_wait))
        request = {"method": method, "data": fields, "retries": retries, "max_wait": limit}
        raw = self._native().call_json(self.token, api_base(),
            json.dumps(request, ensure_ascii=False), uploads, applied_check)
        return _result(raw)

    call = _call

    def probe_set_state(self, name: str) -> tuple[SetState, dict | None]:
        state, pack = json.loads(self._native().probe(name))
        return SetState(state), pack

    def probe_sticker_set(self, name: str) -> tuple[bool, dict | None]:
        state, pack = self.probe_set_state(name)
        return state is not SetState.UNKNOWN, pack

    def live_count_strict(self, name: str) -> int:
        try:
            return _result(self._native().live_count(name))
        except AmbiguousUploadError as error:
            raise LiveStateUnknown(str(error)) from error

    def set_fuids(self, name: str) -> set[str] | None:
        ids = json.loads(self._native().fuids(name))
        return None if ids is None else set(ids)

    def _sticker_matches(self, sticker: dict, source: Path) -> bool | None:
        from emojikit import media
        return self._native().matches(json.dumps(sticker), str(Path(source).absolute()),
                                     media.telegram_sticker_format(sticker), _codec_compare)

    def get_me(self) -> dict:
        return self._call("getMe")

    def get_sticker_set(self, name: str) -> dict:
        return self._call("getStickerSet", data={"name": name})

    def send_message(self, chat_id: int | str, text: str, *, disable_preview: bool = False) -> None:
        self._call("sendMessage", retries=2, data={"chat_id": chat_id, "text": text,
                   "disable_web_page_preview": disable_preview})

    def upload_sticker(self, user_id: int, path: Path) -> str:
        return self._call("uploadStickerFile", data={"user_id": user_id, "sticker_format": "static"},
                         files={"sticker": (path.name, path.read_bytes(), _MIME.get(path.suffix.lower(), "application/octet-stream"))})["file_id"]

    def check_uploadable(self, user_id: int, path: Path, fmt: str) -> None:
        self._call("uploadStickerFile", data={"user_id": user_id, "sticker_format": fmt},
                   files={"sticker": (path.name, path.read_bytes(), _mime_for_path(path))})

    def _mutation(self, user_id, name, path, fmt, emojis, keywords, *, title=None,
                  expected_before=None, needs_repainting=False):
        request = {"user_id": user_id, "name": name, "source": str(Path(path).absolute()),
                   "format": fmt, "emojis": emojis, "keywords": keywords,
                   "create": title is not None, "title": title or "", "expected_before": expected_before,
                   "needs_repainting": needs_repainting}
        limit = None if self.max_flood_wait is None else max(0, int(self.max_flood_wait))
        return _result(self._native().mutate_json(self.token, api_base(), json.dumps(request), _codec_compare, limit))

    def create_set(self, user_id: int, name: str, title: str, png: Path, emoji: str, keywords: str) -> None:
        self._mutation(user_id, name, png, "static", [emoji], keywords.split(","), title=title)

    def add_sticker(self, user_id: int, name: str, png: Path, emoji: str, keywords: str,
                    *, expected_before: int | None = None) -> int | None:
        return self._mutation(user_id, name, png, "static", [emoji], keywords.split(","), expected_before=expected_before)

    def create_emoji_set(self, user_id: int, name: str, title: str, path: Path, fmt: str,
                         emoji_list: list[str], keywords: list[str], *, needs_repainting: bool = False) -> None:
        self._mutation(user_id, name, path, fmt, emoji_list, keywords, title=title, needs_repainting=needs_repainting)

    def add_emoji(self, user_id: int, name: str, path: Path, fmt: str,
                  emoji_list: list[str], keywords: list[str], *, expected_before: int | None = None) -> int | None:
        return self._mutation(user_id, name, path, fmt, emoji_list, keywords, expected_before=expected_before)

    def set_sticker_position(self, file_id: str, position: int) -> None:
        self._call("setStickerPositionInSet", data={"sticker": file_id, "position": int(position)})

    def get_custom_emoji_stickers(self, custom_emoji_ids: list[str]) -> list[dict]:
        if not custom_emoji_ids:
            return []
        if len(custom_emoji_ids) > 200:
            raise ValueError("getCustomEmojiStickers accepts at most 200 IDs per call")
        return self._call("getCustomEmojiStickers", data={"custom_emoji_ids": json.dumps(custom_emoji_ids)})

    def download_bytes(self, file_id: str, retries: int = 5) -> bytes:
        data, envelope = self._native().download(file_id, retries)
        _result(envelope)
        return data

    def download_file(self, file_id: str, dest: Path, retries: int = 5) -> Path:
        data = self.download_bytes(file_id, retries)
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)
        return dest


def _usable_fuids(stickers: list) -> set[str] | None:
    ids = [str(st.get("file_unique_id") or "") for st in stickers]
    return set(ids) if all(ids) and len(set(ids)) == len(ids) else None


def _mime_for_path(path: Path) -> str:
    return _MIME_BY_EXT.get(path.suffix.lower(), "application/octet-stream")


def _trim_keywords(keywords: list[str]) -> list[str]:
    out, total = [], 0
    for raw in keywords:
        key = (raw or "").strip()[:48]
        if not key:
            continue
        if out and total + len(key) + 1 > 60:
            break
        out.append(key)
        total += len(key) + 1
        if len(out) >= 20:
            break
    return out


def _input_sticker(fmt: str, emoji_list: list[str], keywords: list[str]) -> dict:
    return {"sticker": "attach://file0", "format": fmt,
            "emoji_list": [e for e in (emoji_list or []) if e][:20] or [COIN_DEFAULT_EMOJI],
            "keywords": _trim_keywords(keywords)}


def _sticker_json(emoji: str, keywords: str) -> dict:
    return {"sticker": "attach://file0", "format": "static", "emoji_list": [emoji],
            "keywords": _trim_keywords(keywords.split(","))}
