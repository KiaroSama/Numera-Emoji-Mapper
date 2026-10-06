"""Bounded native/source parity and repeated compute timings; optional read-only replay."""

from __future__ import annotations

import argparse
import logging
from pathlib import Path
import random
import sqlite3
import statistics
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from emojikit.logsetup import record_exit_code, setup_logging  # noqa: E402
from emojikit.similarity import greedy_indices, near_indices  # noqa: E402
from tests._similarity_oracle import greedy_indices as reference  # noqa: E402
from tests._similarity_oracle import near_indices as reference_near  # noqa: E402

log = logging.getLogger("benchmark_similarity")


def compare(hashes: list[int]) -> tuple[float, float]:
    samples = [[], []]
    for _ in range(5):
        results = []
        for implementation, timings in zip((reference, greedy_indices), samples, strict=True):
            start = time.perf_counter()
            results.append(implementation(hashes))
            timings.append(time.perf_counter() - start)
        if results[0] != results[1]:
            raise RuntimeError("Similarity order differs from the source oracle")
    return statistics.median(samples[0]), statistics.median(samples[1])


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, help="Optional local catalog, opened read-only")
    args = parser.parse_args(argv)
    setup_logging("benchmark_similarity")
    try:
        if args.catalog:
            uri = args.catalog.resolve().as_uri() + "?mode=ro"
            with sqlite3.connect(uri, uri=True) as db:
                hashes = [int(row[0]) & ((1 << 64) - 1) for row in db.execute(
                    "SELECT phash FROM items WHERE phash IS NOT NULL ORDER BY position, content_key")]
        else:
            rng = random.Random(321)
            hashes = [rng.getrandbits(64) for _ in range(1500)]
        if not hashes or len(hashes) > 10000:
            raise ValueError("Benchmark requires 1..10000 hashes")
        old, new = compare(hashes)
        for query in hashes[:20]:
            for threshold in (-1, 0, 16):
                if near_indices(hashes, query, threshold) != reference_near(hashes, query, threshold):
                    raise RuntimeError("Candidate shortlist differs from the source oracle")
        log.info("Exact parity: hashes=%d python_median=%.6fs rust_median=%.6fs speedup=%.2fx",
                 len(hashes), old, new, old / max(new, 1e-9))
        if len(hashes) >= 100 and new > old:
            raise RuntimeError("Native median regressed against the same-input source baseline")
        return 0
    except (OSError, sqlite3.Error, RuntimeError, ValueError) as exc:
        log.error("Verification failed: %s", exc)
        return 1


if __name__ == "__main__":
    raise SystemExit(record_exit_code(main()))
