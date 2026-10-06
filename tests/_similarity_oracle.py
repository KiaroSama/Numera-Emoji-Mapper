"""Pinned Python compute oracle from 695c895; test-only, never a fallback."""

from __future__ import annotations

import json
import random
from pathlib import Path


def greedy_indices(hashes: list[int]) -> list[int]:
    remaining = list(range(len(hashes)))
    if not remaining:
        return []
    ordered = [remaining.pop(0)]
    values = [hashes[i] for i in remaining]
    last = hashes[ordered[0]]
    while remaining:
        best, best_d = 0, 65
        for i, value in enumerate(values):
            distance = (value ^ last).bit_count()
            if distance < best_d:
                best, best_d = i, distance
                if distance == 0:
                    break
        ordered.append(remaining.pop(best))
        last = values.pop(best)
    return ordered


def near_indices(hashes: list[int | None], query: int, threshold: int) -> list[int]:
    return [i for i, h in enumerate(hashes)
            if threshold >= 0 and h is not None
            and (h ^ query).bit_count() <= threshold]


def fixtures() -> dict:
    rnd = random.Random(1234)
    inputs = [[], [0], [0, 1, 2, 3], [2**64 - 1, 0, 2**63, 1],
              [7, 7, 3, 5, 7], [rnd.getrandbits(64) for _ in range(150)]]
    greedy = [{"hashes": values, "expected": greedy_indices(values)}
              for values in inputs]
    candidates = []
    for values in ([], [None], [None, 0, 1, 2**64 - 1, 2**63, 65535]):
        for query in (0, 2**64 - 1):
            for threshold in (-1, 0, 1, 16):
                candidates.append({"hashes": values, "query": query,
                                   "threshold": threshold,
                                   "expected": near_indices(values, query, threshold)})
    return {"source": "695c895", "greedy": greedy, "near": candidates}


if __name__ == "__main__":
    destination = Path(__file__).parent / "fixtures" / "similarity.json"
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(fixtures(), indent=2) + "\n", encoding="utf-8")
