"""Bind owned temporary/cache storage to a physical application directory."""
from __future__ import annotations

import os
from pathlib import Path
import stat


def _linked(path: Path) -> bool:
    if path.is_symlink():
        return True
    return bool(getattr(path.lstat(), "st_file_attributes", 0) & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0))


def confined_directory(root: Path, relative: str, *, create: bool = True) -> Path:
    root = root.absolute()
    directory = root / relative
    if ".." in Path(relative).parts or Path(relative).is_absolute() or not directory.absolute().is_relative_to(root):
        raise ValueError("Project-local storage escaped its root")
    for part in (directory, *directory.parents):
        if (part.exists() or part.is_symlink()) and _linked(part):
            raise ValueError(f"Linked project-local storage: {relative}")
    if create:
        directory.mkdir(parents=True, exist_ok=True)
    if not directory.resolve().is_relative_to(root.resolve(strict=True)):
        raise ValueError(f"Project-local storage escaped its root: {relative}")
    return directory


def environment(root: Path, original: dict[str, str] | None = None, *, build: bool = False) -> dict[str, str]:
    root = root.absolute()
    for ancestor in (root, *root.parents):
        if (ancestor.exists() or ancestor.is_symlink()) and _linked(ancestor):
            raise ValueError("Project-local storage cannot use a linked/reparse directory")
    root = root.resolve(strict=True)
    paths = {"TEMP": "logs/tmp", "TMP": "logs/tmp", "TMPDIR": "logs/tmp",
             "PIP_CACHE_DIR": ".cache/pip", "XDG_CACHE_HOME": ".cache",
             "PYTHONPYCACHEPREFIX": ".cache/pycache"}
    if build:
        paths.update({"CARGO_HOME": ".cache/cargo", "CARGO_TARGET_DIR": "native/target",
                      "RUSTUP_HOME": ".cache/rustup", "CCACHE_DIR": ".cache/ccache",
                      "SCCACHE_DIR": ".cache/sccache", "HOMEBREW_CACHE": ".cache/homebrew",
                      "HOMEBREW_TEMP": "logs/tmp", "npm_config_cache": ".cache/npm"})
    result = dict(os.environ if original is None else original)
    for key, relative in paths.items():
        result[key] = str(confined_directory(root, relative))
    result.update({"PYTHONUTF8": "1", "PIP_DISABLE_PIP_VERSION_CHECK": "1"})
    return result
