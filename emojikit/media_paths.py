"""How a catalog row's media path is stored, and how every reader turns it back into a file.

`items.file_path` was always absolute. Renaming the project folder left 68 rows pointing at
a folder that no longer existed, and each showed a broken thumbnail in the panel. A path
inside the catalog's data folder is now stored relative to that folder, so the folder can be
renamed or moved; a path outside it (the archive on another drive) stays absolute.

A relative path meant something else before: the old code could store one relative to the
project root. The two readings cannot be told apart from the string, and guessing from which
candidate exists can pick a different file than the one the row meant. So the catalog
records which rule its rows follow (`meta.media_paths`), set once when it converts itself.
"""

from __future__ import annotations

import sqlite3
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent

# The value of meta.media_paths once every in-folder row is relative to the data folder.
DATA_RELATIVE = "data-relative"


def store(data_dir: Path, path: Path) -> str:
    """The string to write: relative (posix) inside `data_dir`, absolute anywhere else."""
    path = Path(path).absolute()
    # PureWindowsPath comparison is case-insensitive, so a differently cased data folder
    # still counts as containing its own media.
    if path.is_relative_to(Path(data_dir).absolute()):
        return path.relative_to(Path(data_dir).absolute()).as_posix()
    return str(path)


def base(con: sqlite3.Connection, data_dir: Path) -> Path:
    """What a relative path in this catalog is relative to, from its own record."""
    row = con.execute("SELECT value FROM meta WHERE key='media_paths'").fetchone()
    return Path(data_dir) if row and row[0] == DATA_RELATIVE else PROJECT_ROOT


def resolve(base_dir: Path, stored: str) -> Path:
    """The file a stored path names. An absolute path is taken as it is."""
    path = Path(stored)
    return path if path.is_absolute() else Path(base_dir) / path
