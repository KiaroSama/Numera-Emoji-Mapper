"""Retained state API backed by native leases, validation and atomic replacement."""
from __future__ import annotations

import contextlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
LOCK_DIR = ROOT / ".locks"
LOCK_STALE_AFTER = 120
_LOCK_BYTE = 4096
_RECORD_WIDTH = 512


class StateInvalid(RuntimeError):
    """Resume state cannot safely drive a mutation."""


class LockBusy(RuntimeError):
    """Native exclusion could not be established."""


def _intent_key(intent) -> str | None:
    return intent.get("key") if isinstance(intent, dict) else intent or None


def validate_state_shape(state: dict, *, base: str, per_set: int) -> None:
    from emojikit import _native
    try:
        _native.state_validate_json(json.dumps(state, ensure_ascii=False), base, per_set)
    except (TypeError, ValueError) as error:
        raise StateInvalid(str(error)) from error


def make_intent(*, key: str, operation: str, set_name: str, set_index: int,
                expected_before: int | None, title: str = "", fmt: str = "static") -> dict:
    from emojikit import _native
    fields = {"key": key, "operation": operation, "set_name": set_name,
              "set_index": set_index, "expected_before": expected_before,
              "title": title, "format": fmt}
    return json.loads(_native.state_intent_json(json.dumps(fields, ensure_ascii=False)))


def pack_family_lock_path(base: str) -> Path:
    safe = re.sub(r"[^A-Za-z0-9_-]+", "_", base).strip("_") or "default"
    return LOCK_DIR / f"pack_{safe}.lock"


def canonical_map_lock():
    return exclusive_lock(LOCK_DIR / "canonical_map.lock")


@contextlib.contextmanager
def exclusive_lock(path: Path, *, stale_after: float = LOCK_STALE_AFTER, on_acquired=None):
    from emojikit import _native
    # stale_after remains a compatibility argument; the OS lease, never age, decides ownership.
    try:
        lease = _native.NativeStateLease(str(Path(path)), _utc_now())
    except (OSError, RuntimeError) as error:
        raise LockBusy(str(error)) from error
    descriptor = None
    try:
        if on_acquired is not None:
            descriptor = lease.descriptor()
            on_acquired(descriptor)
        yield lease.heartbeat
    finally:
        try:
            if descriptor is not None:
                os.close(descriptor)
        finally:
            lease.close()


def _utc_now() -> str:
    from datetime import datetime, timezone
    return datetime.now(tz=timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")


def write_json_atomic(path: Path, data) -> None:
    from emojikit import _native
    # Serialization is binding glue: Rust exclusively owns staging/fsync/permissions/replacement.
    text = json.dumps(data, ensure_ascii=False, indent=1)
    # The original text stream translated line endings on Windows before publishing bytes.
    encoded = text.replace("\n", os.linesep).encode("utf-8")
    _native.state_write_bytes(str(Path(path)), encoded)
