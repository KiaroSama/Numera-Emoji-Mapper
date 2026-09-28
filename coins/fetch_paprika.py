"""Fetch logos for still-missing inventory coins from CoinPaprika.

CoinGecko search failed for ~29 obscure inventory coins. CoinPaprika
(free, no API key) covers many of them. This script searches CoinPaprika by
coin name (and ticker as fallback), picks a confident match, downloads the logo
from CoinPaprika's static CDN, converts it to a 100x100 emoji PNG, optionally
adds it to the last not-full Telegram pack, captures the new custom_emoji_id,
and re-fills the inventory.

Key design points:
- The logo URL is deterministic: https://static.coinpaprika.com/coin/{id}/logo.png
  so once the coin id is known from search, the image is fetched from the static
  CDN (not the metered API) -> only /search calls count against the free quota.
- The free /search quota is small and returns HTTP 402 when exhausted. On 402 the
  script stops cleanly and persists progress to paprika_matches.json, so a later
  run (after the quota window resets) resumes without re-searching resolved coins.
- Matching is conservative: only "name-exact" or "symbol" matches are accepted.
  Weaker matches are reported but NOT used (a wrong logo is worse than a blank).

Usage:
  python fetch_paprika.py --dry     # search + cache matches, no downloads/packs
  python fetch_paprika.py           # download cached/confident matches, add to packs
"""

from __future__ import annotations

# This script lives in coins/; allow importing the shared engine (build_pack.py)
# from the project root.
import os as _bootstrap_os
import sys as _bootstrap_sys
_bootstrap_sys.path.insert(0, _bootstrap_os.path.dirname(
    _bootstrap_os.path.dirname(_bootstrap_os.path.abspath(__file__))))

import argparse
import json
import time
from pathlib import Path

from emojikit.build_pack import (EXIT_PARTIAL, ingest_exit_code, load_env)
from emojikit.packstate import write_json_atomic
from emojikit.telegram_api import Telegram
from emojikit.logsetup import setup_logging
from coins import _inventory
from coins._paprika_api import (QUOTA_EXHAUSTED, SLEEP, TRANSIENT, http_bytes, http_json,
                                search_match, to_emoji_png)
# Staging, recovery and the verified publisher are shared with fetch_cmc; one
# definition of the state paths lives there, and this script reads it back.
from coins._provider_publish import TICKER_IDS, incoming_dir, publish_logos
from coins._env import require_token
from emojikit import operator_config

ROOT = Path(__file__).resolve().parent
INV = ROOT / "currency-emoji-inventory.md"
OUT_INV = ROOT / "currency-emoji-inventory.filled.md"
CACHE = ROOT / "paprika_matches.json"  # resumable {ticker: {id, conf, name}}

COIN = "https://api.coinpaprika.com/v1/coins/"
LOGO_CDN = "https://static.coinpaprika.com/coin/{id}/logo.png"


# fetch_cmc calls these names and the tests retarget the inventory by patching fp.INV, so
# the paths have to be read at call time, not at import.
def parse_missing(have: set[str]) -> list[tuple[str, str]]:
    return _inventory.parse_missing(have, INV)


def refill_inventory(ticker_to_id: dict[str, str]) -> tuple[int, int]:
    return _inventory.refill_inventory(ticker_to_id, INV, OUT_INV)


def load_cache() -> dict[str, dict]:
    if CACHE.is_file():
        return json.loads(CACHE.read_text("utf-8"))
    return {}


def save_cache(cache: dict[str, dict]) -> None:
    write_json_atomic(CACHE, cache)


def resolve_phase(missing, cache, retry_unmatched: bool = False) -> tuple[bool, int]:
    """Search CoinPaprika for unresolved coins. Returns (quota_hit, unknown).

    ``unknown`` counts coins whose search did not answer. They are left OUT of
    the cache: a cached ``{"id": None}`` is skipped by every later run, so one
    outage would mark each coin searched during it unresolvable for good.
    ``retry_unmatched`` re-queries cached misses, which may be outage results
    written before this distinction existed.
    """
    quota_hit = False
    unknown = 0
    for name, tk in missing:
        if tk in cache and not (retry_unmatched and cache[tk].get("id") is None):
            continue
        r = search_match(name, tk)
        if r is QUOTA_EXHAUSTED:
            print("  HTTP 402: CoinPaprika free quota exhausted; stopping search.",
                  flush=True)
            quota_hit = True
            break
        if r is TRANSIENT:
            unknown += 1
            print(f"  unknown {tk:12s}  provider unreachable; not cached  ({name})",
                  flush=True)
            continue
        cid, conf = r
        if cid and conf in ("name-exact", "symbol"):
            cache[tk] = {"id": cid, "conf": conf, "name": name}
            print(f"  MATCH {tk:12s} <- {cid}  [{conf}]  ({name})", flush=True)
        else:
            cache[tk] = {"id": None, "conf": conf, "name": name}
            tag = "partial" if conf.startswith("partial") else "none"
            print(f"  {tag:7s} {tk:12s}  {conf}  ({name})", flush=True)
        save_cache(cache)  # persist after every coin (resumable)
    return quota_hit, unknown


def main(argv: list[str] | None = None) -> int:
    # argparse, not `"--dry" in sys.argv`: membership testing means every
    # spelling that is not exactly "--dry" -- a typo like --dryy, an unknown
    # flag, a stray positional -- silently selected the LIVE branch, which
    # uploads to Telegram with the owner's credentials and rewrites the
    # canonical map. The safe reading of an argument nobody recognises is to
    # refuse, and argparse refuses with a usage error and exit 2.
    ap = argparse.ArgumentParser(
        description="Fill unmapped coins from CoinPaprika and publish their "
                    "logos to the coin pack family.",
        # allow_abbrev=False: argparse accepts unambiguous prefixes by
        # default, so "--dr" silently became "--dry". A near-miss is the
        # typo class this guard exists for -- it must not be guessed at,
        # in either direction.
        allow_abbrev=False)
    ap.add_argument("--dry", action="store_true",
                    help="resolve and report only; download nothing and make "
                         "no pack or map changes.")
    ap.add_argument("--retry-unmatched", action="store_true",
                    help="search again for coins cached as unmatched; older "
                         "runs cached provider outages that way.")
    args = ap.parse_args(argv)
    dry = args.dry
    load_env()
    operator_config.stop_unless("COIN_PACK_BASE", "COIN_PACK_TITLE")
    # Before the search and the downloads, not after them: a dry run publishes
    # nothing, so only a live run needs the token.
    token = None if dry else require_token("TELEGRAM_BOT_TOKEN")
    setup_logging("fetch_paprika")
    ticker_to_id: dict[str, str] = json.loads(TICKER_IDS.read_text("utf-8"))
    have = set(ticker_to_id)
    missing = parse_missing(have)
    cache = load_cache()
    print(f"missing to resolve: {len(missing)} | cached: {len(cache)} | dry={dry}",
          flush=True)

    quota_hit, unknown = resolve_phase(missing, cache, args.retry_unmatched)

    def finish(code: int) -> int:
        # A coin whose search went unanswered is still to do: never a clean exit.
        return max(code, EXIT_PARTIAL) if unknown else code

    confident = [(tk, v["id"]) for tk, v in cache.items()
                 if v.get("id") and tk not in have]
    print(f"\nconfident(new)={len(confident)} cached_total={len(cache)} "
          f"quota_hit={quota_hit} unknown={unknown}", flush=True)

    if dry:
        print("dry run: matches cached, no downloads/pack changes.", flush=True)
        return finish(0)
    if not confident:
        print("no new confident matches to add.", flush=True)
        return finish(0)

    # Download confident matches from the static CDN (not metered) -> emoji PNGs.
    # If the deterministic CDN path is missing (404), fall back to the /coins/{id}
    # API 'logo' field (metered, but quota is available when this path is reached).
    fetched: list[str] = []
    failed = 0
    for tk, cid in confident:
        data = http_bytes(LOGO_CDN.format(id=cid))
        if not data:
            detail = http_json(COIN + cid)
            time.sleep(SLEEP)
            url = detail.get("logo") if isinstance(detail, dict) else None
            if url and url != LOGO_CDN.format(id=cid):
                data = http_bytes(url)
        if not data:
            print(f"  download failed: {tk} ({cid})", flush=True)
            failed += 1
            continue
        if to_emoji_png(data, incoming_dir() / f"{tk}.png"):
            fetched.append(tk)
            print(f"  got logo: {tk} <- {cid}", flush=True)
        else:
            print(f"  unusable logo: {tk} ({cid})", flush=True)
            failed += 1

    print(f"fetched logos: {len(fetched)}", flush=True)
    if not fetched:
        print("nothing to add.", flush=True)
        return finish(ingest_exit_code(0, failed))

    # Add to the last not-full set, overflow to new sets.
    tg = Telegram(token)
    added, add_failed = publish_logos(tg, fetched, ticker_to_id)
    filled, total = refill_inventory(ticker_to_id)
    print(f"added {added} stickers; inventory filled: {filled}/{total}", flush=True)
    return finish(ingest_exit_code(added, failed + add_failed))


if __name__ == "__main__":
    raise SystemExit(main())
