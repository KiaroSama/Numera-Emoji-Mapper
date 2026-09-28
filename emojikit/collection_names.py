"""Pack naming rules: the base, the 64-character set name, the format list.

Split out of `build_collection.py` (file-size limit); it re-exports these names.
"""

from __future__ import annotations

import re

from emojikit.collection_state import FMT_TAG


_BASE_RE = re.compile(r"[A-Za-z][A-Za-z0-9]*(?:_[A-Za-z0-9]+)*")
# The longest suffix a base can pick up: one format letter, a set index, and
# "_by_" plus the bot username. Checked against the real username at publish
# time; this is the static part.
_NAME_MAX = 64


def valid_base(base: str) -> str:
    """The chosen part of a set name, checked against Telegram's rule.

    Underscores are allowed -- this used to reject them, which is stricter than
    Telegram and refuses a perfectly legal name like YourBrand_Emoji_Packs.
    Consecutive underscores are not, and neither is a trailing one, because
    "<base>_" + "1_by_..." is fine but "<base>_" + "_by_..." is not, and the
    rule is easier to hold as "single underscores between parts".
    """
    if not _BASE_RE.fullmatch(base):
        raise SystemExit(
            "ERROR: --base must begin with a letter and contain only letters, "
            "digits and single underscores between them (Telegram's rule for a "
            "sticker-set name). Examples: 'mypack', 'YourBrand_Emoji_Packs'.")
    return base


def check_name_length(base: str, bot: str, tag: str = "") -> None:
    """Refuse a base that cannot fit Telegram's 64-character set name.

    Caught here rather than as a Bot API error on the first upload, which is
    after the plan is frozen and the run has already started.
    """
    longest = f"{base}{tag}999_by_{bot}"
    if len(longest) > _NAME_MAX:
        raise SystemExit(
            f"ERROR: --base '{base}' is too long: the set name would reach "
            f"{len(longest)} characters ('{longest}') and Telegram allows "
            f"{_NAME_MAX}. Shorten --base by {len(longest) - _NAME_MAX}.")


def parse_formats(raw: str) -> list[str]:
    """Validate --formats. Dropping unknown values silently made ``--formats
    garbage`` a successful run that published nothing at all."""
    parts = [f.strip() for f in raw.split(",")]
    if any(f not in FMT_TAG for f in parts) or len(set(parts)) != len(parts):
        raise ValueError(f"--formats must be a comma list of "
                         f"{'/'.join(FMT_TAG)} with no duplicates or blanks; "
                         f"got {raw!r}.")
    return parts
