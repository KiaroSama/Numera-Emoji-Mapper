"""Stop a coin tool before any network work when its token is unset.

The fetchers read the bot token only after their whole download phase, and
two tools read it as ``os.environ[...]``: an unset or misnamed token cost
minutes of downloading and then ended in a bare KeyError traceback.
"""

from __future__ import annotations

import os
import sys

from emojikit.cli_env import EXIT_USAGE


def require_token(name: str) -> str:
    """The value of environment variable ``name``, or exit 2 naming it."""
    value = os.environ.get(name, "").strip()
    if not value:
        print(f"{name} is not set (see .env.example)", file=sys.stderr, flush=True)
        raise SystemExit(EXIT_USAGE)
    return value
