"""Exact media codecs and native-backed shared APIs for the Rust local backend.

Application commands run through the installed numera-emoji executable.
Retained coin tools use these codec, transport, state and logging boundaries;
development helpers and test references are not production fallback applications.
"""

from __future__ import annotations

__version__ = "2.0.0"

__all__ = ["logsetup", "media"]
