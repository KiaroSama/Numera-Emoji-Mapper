"""Required native hash computations; identity decisions stay with the catalog."""

from __future__ import annotations

import importlib


def require_native():
    try:
        native = importlib.import_module("emojikit._native")
    except ImportError as exc:
        raise RuntimeError("Required Rust computation module is unavailable. "
                           "Run: python scripts/build_native.py") from exc
    if (getattr(native, "API_VERSION", None) != 1
            or not callable(getattr(native, "greedy_indices", None))
            or not callable(getattr(native, "near_indices", None))):
        raise RuntimeError("Rust computation module is incompatible. "
                           "Rebuild: python scripts/build_native.py")
    return native


def greedy_indices(hashes: list[int]) -> list[int]:
    return require_native().greedy_indices(hashes)


def near_indices(hashes: list[int | None], query: int, threshold: int) -> list[int]:
    return require_native().near_indices(hashes, query, threshold)
