"""Validate an extracted prebuilt package before activating its native runtime."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path, PurePosixPath
import platform
import sys
import tomllib


def verify(root: Path) -> dict:
    manifest_path = root / "package.json"
    if manifest_path.is_symlink():
        raise ValueError("Linked package manifest is not supported")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    root = root.resolve(strict=True)
    if not isinstance(manifest, dict) or manifest.get("schema") != 1:
        raise ValueError("Unsupported package manifest")
    system = {"Windows": "windows", "Linux": "linux", "Darwin": "macos"}.get(platform.system())
    arch = {"amd64": "x64", "x86_64": "x64", "arm64": "arm64", "aarch64": "arm64"}.get(platform.machine().lower())
    if not system or not arch or manifest.get("platform") != f"{system}-{arch}":
        raise ValueError("Download the package matching this operating system and CPU")
    if sys.version_info[:2] != (3, 11):
        raise ValueError("This offline package needs Python 3.11 (matching its bundled dependency wheels)")
    if system == "linux":
        libc, version = platform.libc_ver()
        try:
            supported = libc == "glibc" and tuple(map(int, version.split(".")[:2])) >= (2, 39)
        except ValueError:
            supported = False
        if not supported:
            raise ValueError("This Linux package needs glibc 2.39+ (Ubuntu 24.04+); Alpine is not supported")
    if system == "macos" and int(platform.mac_ver()[0].split(".")[0]) < 15:
        raise ValueError("This macOS package needs macOS 15+")
    files = manifest.get("files")
    if not isinstance(files, dict) or not files:
        raise ValueError("Package manifest has no file inventory")
    for name, expected in files.items():
        relative = PurePosixPath(name)
        if relative.is_absolute() or any(part in {".", ".."} for part in relative.parts) or "\\" in name or ":" in name:
            raise ValueError("Invalid package member path")
        path = root / relative
        if any(parent.is_symlink() for parent in (path, *path.parents) if parent != root.parent):
            raise ValueError(f"Linked package member: {name}")
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError(f"Missing or changed package member: {name}")
    version = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    wheel = manifest.get("wheel")
    if manifest.get("version") != version or not isinstance(wheel, str) or Path(wheel).name != wheel:
        raise ValueError("Package version or wheel is inconsistent")
    if f"wheels/{wheel}" not in files or not wheel.startswith(f"numera_emoji_core-{version}-") or not wheel.endswith(".whl"):
        raise ValueError("Matching native wheel is absent")
    executable = "native/runtime/numera-emoji.exe" if system == "windows" else "native/runtime/numera-emoji"
    required = {executable, "assets/panel.html", "requirements.txt", "scripts/launch_prebuilt.py"}
    if not required.issubset(files):
        raise ValueError("Package is incomplete")
    return manifest
