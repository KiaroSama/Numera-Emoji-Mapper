"""JSON that is safe inside an inert ``<script type="application/json">`` block."""

from __future__ import annotations

import json


def json_for_script(value, *, indent: int | None = None) -> str:
    """``value`` as JSON that cannot end the ``<script>`` block it sits in.

    ``</script>`` inside a label -- and labels come from downloaded packs, so
    they are attacker-influenced -- would otherwise close the block, and
    everything after it runs as markup and script when the page is opened.
    ``<`` and ``>`` become ``\\u003c``/``\\u003e`` (``JSON.parse`` turns them
    back); U+2028/U+2029 are escaped because they are line terminators in a JS
    string.
    """
    return (json.dumps(value, ensure_ascii=False, indent=indent)
            .replace("<", "\\u003c").replace(">", "\\u003e")
            .replace("\u2028", "\\u2028").replace("\u2029", "\\u2029"))
