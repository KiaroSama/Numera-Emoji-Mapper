"""Shared text of what the panel serves, for the served-text page suites.

``PAGE`` is the HTML, ``SCRIPT`` the page's scripts concatenated in load order.
No test classes live here, so importing it never runs a test twice.
"""

from __future__ import annotations




from tests.reference import panel as p

PAGE = p.PAGE
SCRIPT = p.SCRIPT


def block(src: str, start: str, end: str) -> str:
    """The text from the first ``start`` up to the next ``end`` after it."""
    i = src.index(start)
    return src[i:src.index(end, i)]
