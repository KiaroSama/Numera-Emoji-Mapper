"""Fetch logos for the last few inventory coins via the CoinMarketCap Pro API.

These coins are unavailable (no logo) on CoinGecko and CoinPaprika. CMC's free
Basic plan covers them. For each still-missing inventory ticker:
  1) /v1/cryptocurrency/map  -> candidate ids by symbol (base + full ticker)
  2) pick a confident match (name-exact or symbol) -- conservative, like the
     CoinPaprika flow (a wrong logo is worse than a blank entry)
  3) /v2/cryptocurrency/info -> logo url; download the 128x128 variant
  4) convert to a 100x100 emoji PNG, add to the last not-full pack, capture the
     new custom_emoji_id, and re-fill the inventory.

The CMC API key is read from .env as CMC_API_KEY (never printed).

Usage:
  python fetch_cmc.py --dry   # search + report candidates only
  python fetch_cmc.py         # download confident matches, add to packs
"""

from __future__ import annotations

import argparse
import json
import os
import time
import urllib.parse

from emojikit.cli_env import (EXIT_PARTIAL, ingest_exit_code, load_env)
from emojikit import operator_config
from emojikit.telegram_api import (Telegram)
from emojikit.logsetup import setup_logging
from coins import _http
from coins._inventory import base_ticker
# Reuse proven helpers -- including the ONE verified publisher, so this
# fetcher cannot drift back into its own copy. Package-qualified so the module
# also imports as ``coins.fetch_cmc``.
from coins._paprika_api import TRANSIENT, classify, http_bytes, to_emoji_png
from coins._provider_publish import TICKER_IDS, incoming_dir, publish_logos
from coins.fetch_paprika import parse_missing, refill_inventory
from coins._env import require_token

MAP = "https://pro-api.coinmarketcap.com/v1/cryptocurrency/map?symbol="
INFO = "https://pro-api.coinmarketcap.com/v2/cryptocurrency/info?id="
SLEEP = 1.0


def cmc_headers() -> dict:
    key = os.environ.get("CMC_API_KEY", "").strip()
    if not key:
        raise SystemExit("CMC_API_KEY missing in .env")
    return {"X-CMC_PRO_API_KEY": key, "Accept": "application/json"}


def cmc_json(url: str, headers: dict, retries: int = 4):
    """Decoded body, None for "no such symbol", or TRANSIENT when CMC never answered.

    401/403 stop the whole run: a rejected key is not "no match", and reporting
    it as one made a dead key look like a clean run with nothing to add.
    """
    r = _http.get(url, headers=headers, retries=retries,
                  stop_on=(400, 401, 403, 404))
    if r is None:
        return TRANSIENT
    if r.status_code in (401, 403):
        raise SystemExit(f"CMC_API_KEY rejected by CoinMarketCap "
                         f"(HTTP {r.status_code})")
    if r.status_code in (400, 404):
        return None  # symbol not found / bad request -> no match
    return r.json()


def map_candidates(headers: dict, name: str, ticker: str):
    """(candidates [{id,name,symbol}] for base+full ticker, any lookup transient)."""
    seen: dict[int, dict] = {}
    transient = False
    for sym in {ticker.upper(), base_ticker(ticker).upper()}:
        d = cmc_json(MAP + urllib.parse.quote(sym), headers)
        time.sleep(SLEEP)
        if d is TRANSIENT:
            transient = True
            continue
        for c in (d or {}).get("data", []) or []:
            seen[c["id"]] = {"id": str(c["id"]), "name": c.get("name", ""),
                             "symbol": c.get("symbol", "")}
    return list(seen.values()), transient


def get_logo_url(headers: dict, cmc_id: str) -> str | None:
    d = cmc_json(INFO + cmc_id, headers)
    time.sleep(SLEEP)
    if not isinstance(d, dict):
        return None
    info = (d.get("data") or {}).get(cmc_id) or {}
    logo = info.get("logo")
    if not logo:
        return None
    # Prefer a larger variant; CMC serves 64x64 by default.
    return logo.replace("/64x64/", "/128x128/")


def main(argv: list[str] | None = None) -> int:
    # See fetch_paprika.main(): `"--dry" in sys.argv` let any misspelling fall
    # through to the live branch, which spends the CMC quota and publishes to
    # Telegram. argparse refuses an unrecognised argument instead.
    ap = argparse.ArgumentParser(
        description="Fill unmapped coins from CoinMarketCap and publish their "
                    "logos to the coin pack family.",
        # allow_abbrev=False: argparse accepts unambiguous prefixes by
        # default, so "--dr" silently became "--dry". A near-miss is the
        # typo class this guard exists for -- it must not be guessed at,
        # in either direction.
        allow_abbrev=False)
    ap.add_argument("--dry", action="store_true",
                    help="resolve and report only; download nothing and make "
                         "no pack or map changes.")
    dry = ap.parse_args(argv).dry
    load_env()
    operator_config.stop_unless("COIN_PACK_BASE", "COIN_PACK_TITLE")
    headers = cmc_headers()
    # Before the lookups and downloads; a dry run publishes nothing.
    token = None if dry else require_token("TELEGRAM_BOT_TOKEN")
    setup_logging("fetch_cmc")
    ticker_to_id: dict[str, str] = json.loads(TICKER_IDS.read_text("utf-8"))
    have = set(ticker_to_id)
    missing = parse_missing(have)
    print(f"missing to resolve via CMC: {len(missing)} | dry={dry}", flush=True)

    resolved: list[tuple[str, str]] = []  # (ticker, cmc_id)
    unknown = 0
    for name, tk in missing:
        cands, transient = map_candidates(headers, name, tk)
        cid, conf = classify(cands, name, tk)
        if cid and conf in ("name-exact", "symbol"):
            resolved.append((tk, cid))
            print(f"  MATCH {tk:12s} <- cmc:{cid}  [{conf}]  ({name})", flush=True)
        elif transient:
            # Unknown, not "no match": the lookup never got an answer.
            unknown += 1
            print(f"  unknown {tk:12s}  provider unreachable  ({name})", flush=True)
        else:
            print(f"  none  {tk:12s}  {conf}  ({name})", flush=True)

    def finish(code: int) -> int:
        return max(code, EXIT_PARTIAL) if unknown else code

    print(f"\nconfident={len(resolved)} unknown={unknown}", flush=True)
    if dry or not resolved:
        if dry:
            print("dry run: no downloads/pack changes.", flush=True)
        return finish(0)

    # Resolve logos + build emoji PNGs.
    fetched: list[str] = []
    failed = 0
    for tk, cid in resolved:
        url = get_logo_url(headers, cid)
        if not url:
            print(f"  no logo: {tk} (cmc:{cid})", flush=True)
            failed += 1
            continue
        data = http_bytes(url)
        if not data:  # 128x128 may not exist; fall back to default 64x64
            data = http_bytes(url.replace("/128x128/", "/64x64/"))
        if not data:
            print(f"  download failed: {tk}", flush=True)
            failed += 1
            continue
        if to_emoji_png(data, incoming_dir() / f"{tk}.png"):
            fetched.append(tk)
            print(f"  got logo: {tk} <- cmc:{cid}", flush=True)
        else:
            print(f"  unusable logo: {tk} (cmc:{cid})", flush=True)
            failed += 1

    print(f"fetched logos: {len(fetched)}", flush=True)
    if not fetched:
        print("nothing to add.", flush=True)
        return finish(ingest_exit_code(0, failed))

    # Same verified publisher as fetch_paprika: locked, duplicate-proof adds and
    # emoji ids read by identity.
    tg = Telegram(token)
    added, add_failed = publish_logos(tg, fetched, ticker_to_id)
    filled, total = refill_inventory(ticker_to_id)
    print(f"added {added} stickers; inventory filled: {filled}/{total}", flush=True)
    return finish(ingest_exit_code(added, failed + add_failed))


if __name__ == "__main__":
    raise SystemExit(main())
