"""Build keywords.csv from the logos already on disk.

Uses CoinGecko's lightweight /coins/list endpoint (a single request, no
pagination) to map each ticker to a full coin name for the keywords column,
then lists every SVG and PNG logo present in logos/.
"""

from __future__ import annotations

# This script lives in coins/; allow importing the shared HTTP client whether it
# is run as ``python coins/build_keywords.py`` or imported from the project root.

import time
from pathlib import Path

from coins import _http
from coins._keywords import write_keywords_csv
from emojikit.cli_env import EXIT_PARTIAL
from emojikit.logsetup import setup_logging

ROOT = Path(__file__).resolve().parent
SVG_DIR = ROOT / "logos" / "svg"
PNG_DIR = ROOT / "logos" / "png"
OUT = ROOT / "keywords.csv"
LIST_URL = "https://api.coingecko.com/api/v3/coins/list"
MARKETS_URL = "https://api.coingecko.com/api/v3/coins/markets"
MARKETS_PAGES = 12   # market-cap order: gives canonical names for the top coins
HEADERS = {"User-Agent": "Mozilla/5.0 (logo-fetcher; local tool)"}
PAGE_DELAY = _http.page_delay()   # COIN_PAGE_DELAY overrides; see coins/_http.py


def _get_json(url: str, *, retries: int = 6):
    resp = _http.get(url, headers=HEADERS, timeout=60, retries=retries,
                     backoff=8.0, max_backoff=40.0)
    if resp is None:
        raise RuntimeError(f"GET failed after {retries} attempts: {url}")
    return resp.json()


def fetch_canonical_names() -> tuple[dict[str, str], bool]:
    """(ticker(lower) -> canonical coin name, whether every lookup answered).

    Prefers market-cap order (the most prominent coin for each ticker), then
    fills the long tail from the full /coins/list.
    """
    names: dict[str, str] = {}
    complete = True
    # 1) Market-cap order first (highest cap wins per ticker -> canonical name).
    for page in range(1, MARKETS_PAGES + 1):
        url = (f"{MARKETS_URL}?vs_currency=usd&order=market_cap_desc&per_page=250"
               f"&page={page}&sparkline=false")
        try:
            data = _get_json(url)
        except RuntimeError as exc:
            print(f"  markets page {page} unavailable: {exc}", flush=True)
            complete = False
            break
        if not isinstance(data, list) or not data:
            break
        for c in data:
            sym = str(c.get("symbol", "")).lower().strip()
            name = str(c.get("name", "")).strip()
            if sym and sym not in names:
                names[sym] = name
        print(f"  markets page {page}: {len(names)} canonical names so far.", flush=True)
        time.sleep(PAGE_DELAY)
    # 2) Long tail from /coins/list (only fills tickers not already set).
    try:
        for c in _get_json(LIST_URL):
            sym = str(c.get("symbol", "")).lower().strip()
            name = str(c.get("name", "")).strip()
            if sym and sym not in names:
                names[sym] = name
    except RuntimeError as exc:
        print(f"  coins/list unavailable: {exc}", flush=True)
        complete = False
    print(f"name map: {len(names)} entries.", flush=True)
    return names, complete


def main() -> int:
    setup_logging("build_keywords")
    names, complete = fetch_canonical_names()
    rows: dict[str, dict] = {}
    for p in sorted(SVG_DIR.glob("*.svg")):
        t = p.stem.lower()
        rows[t] = {"ticker": t, "name": names.get(t, ""), "format": "svg",
                   "file": f"logos/svg/{t}.svg"}
    for p in sorted(PNG_DIR.glob("*.png")):
        t = p.stem.lower()
        rows.setdefault(t, {"ticker": t, "name": names.get(t, ""), "format": "png",
                            "file": f"logos/png/{t}.png", "path": p})

    # The shared writer drops PNGs that do not decode -- the fetch_logos fix
    # this script used to undo -- and keeps names this run could not look up.
    write_keywords_csv(rows, OUT)

    svg = sum(1 for r in rows.values() if r["format"] == "svg")
    png = sum(1 for r in rows.values() if r["format"] == "png")
    named = sum(1 for r in rows.values() if r["name"])
    print(f"keywords.csv written: {len(rows)} logos ({svg} svg, {png} png), {named} with names.",
          flush=True)
    if not complete:
        print("INCOMPLETE: a name lookup failed; earlier names were kept. "
              "Re-run to refresh them.", flush=True)
        return EXIT_PARTIAL
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
