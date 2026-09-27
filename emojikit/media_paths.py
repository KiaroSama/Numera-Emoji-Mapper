"""How a catalog row's media path is stored, and how every reader turns it back into a file.

`items.file_path` was always absolute. Renaming the project folder left 68 rows pointing at
a folder that no longer existed, and each showed a broken thumbnail in the panel. A path
inside the catalog's data folder is now stored relative to that folder, so the folder can be
renamed or moved; a path outside it (the archive on another drive) stays absolute.

A relative path meant something else before: the old code could store one relative to the
project root. So a data-folder path carries its own marker, a leading `./`, which the old
code could never have written (`str(Path("./x"))` is `x`). Each row says which rule it
follows; nothing is inferred from which candidate file happens to exist, and nothing
depends on a flag stored elsewhere that could be lost.
"""

from __future__ import annotations

from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent

# Marks a path relative to the catalog's data folder.
DATA_PREFIX = "./"


def store(data_dir: Path, path: Path) -> str:
    """The string to write: `./<posix>` inside `data_dir`, absolute anywhere else."""
    path, data_dir = Path(path).absolute(), Path(data_dir).absolute()
    # PureWindowsPath comparison is case-insensitive, so a differently cased data folder
    # still counts as containing its own media.
    if path.is_relative_to(data_dir):
        return DATA_PREFIX + path.relative_to(data_dir).as_posix()
    return str(path)


def resolve(data_dir: Path, stored: str, project_root: Path = PROJECT_ROOT) -> Path:
    """The file a stored path names.

    `./...` is inside `data_dir`; an absolute path is taken as it is; any other relative
    path is the old project-root shape.
    """
    if stored.startswith(DATA_PREFIX):
        return Path(data_dir) / stored[len(DATA_PREFIX):]
    path = Path(stored)
    return path if path.is_absolute() else Path(project_root) / path
