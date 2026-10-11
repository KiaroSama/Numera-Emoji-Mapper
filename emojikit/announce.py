"""Retained coin announcement API; Rust owns routing, rendering and network calls."""
from __future__ import annotations

import json
import os
from pathlib import Path

from emojikit.telegram_api import Telegram, api_base

ROOT = Path(__file__).resolve().parents[1]


def links_chat_id(owner_id: int) -> str | int:
    raw = os.environ.get("PACK_LINKS_CHAT_ID", "").strip()
    return owner_id if not raw else raw if raw.startswith("@") else int(raw)


def worker_publish_url() -> str:
    return os.environ.get("WORKER_PUBLISH_URL", "").strip()


def _announce(token: str, owner_id: int, packs: list[dict], *, bot: str,
              note: str, style: str, worker_only: bool = False, timeout: float = 30) -> str:
    from emojikit import _native
    request = {"owner": owner_id, "packs": packs, "bot": bot, "note": note,
               "style": style, "worker_only": worker_only, "timeout": timeout}
    return _native.announce_packs_json(str(ROOT), token, api_base(),
                                     json.dumps(request, ensure_ascii=False))


def announce_packs(tg: Telegram, owner_id: int, packs: list[dict], *,
                   bot: str, note: str = "", style: str = "cards") -> str:
    return _announce(tg.token, owner_id, packs, bot=bot, note=note, style=style)


def announce_via_worker(packs: list[dict], *, note: str = "", bot: str = "coin",
                        style: str = "cards", timeout: int = 30) -> None:
    _announce("", 0, packs, bot=bot, note=note,
              style=style, worker_only=True, timeout=timeout)
