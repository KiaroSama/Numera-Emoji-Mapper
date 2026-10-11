"""Build a public prebuilt archive on CI; never collect arbitrary checkout files."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
PLATFORMS = {"Windows": "windows", "Linux": "linux", "Darwin": "macos"}


def platform_id() -> str:
    system = PLATFORMS.get(platform.system())
    machine = platform.machine().lower()
    architecture = {"amd64": "x64", "x86_64": "x64", "arm64": "arm64", "aarch64": "arm64"}.get(machine)
    if not system or not architecture:
        raise ValueError("Unsupported package platform or architecture")
    return f"{system}-{architecture}"


def package(root: Path, executable: Path, wheel: Path, dependencies: Path, output: Path, revision: str) -> Path:
    version = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))["project"]["version"]
    target = platform_id()
    if executable.is_symlink() or not executable.is_file() or wheel.is_symlink() or not wheel.is_file():
        raise ValueError("A regular native executable and wheel are required")
    if wheel.suffix != ".whl" or not wheel.name.startswith(f"numera_emoji_core-{version}-"):
        raise ValueError("Wheel version does not match this application")
    from scripts.local_environment import environment
    os.environ.update(environment(root))
    if not output.absolute().resolve().is_relative_to(root.resolve()):
        raise ValueError("Package output must stay inside its owning project")
    output.mkdir(parents=True, exist_ok=True)
    if output.is_symlink():
        raise ValueError("Linked package output is not supported")
    stem = f"numera-emoji-mapper-{version}-{target}"
    destination = output / f"{stem}.zip"
    if destination.exists():
        raise FileExistsError(f"Archive already exists: {destination.name}")
    with tempfile.TemporaryDirectory(prefix="package-", dir=output) as temp:
        stage = Path(temp) / stem
        stage.mkdir()
        members = [Path(name) for name in ("LICENSE", "README.md", "run.ps1", "requirements.txt", "requirements-coins.txt", "pyproject.toml", ".env.example")]
        # Source allowlist: caches, operator media/config and test references never enter.
        members += [p.relative_to(root) for folder, pattern in (("emojikit", "*.py"), ("assets", "*"), ("coins", "*.py"))
                    for p in (root / folder).glob(pattern) if p.is_file()]
        members += [Path("coins") / name for name in ("keywords.csv", "run_convert.ps1", "run_rebuild_loop.ps1")]
        members += [Path("scripts") / name for name in ("install_prebuilt.py", "prebuilt_manifest.py", "launch_prebuilt.py", "local_environment.py", "build_native.py", "build_job.py", "run-actions.ps1", "run-native.ps1", "native_convert_owner.py")]
        for relative in members:
            source = root / relative
            if source.is_symlink() or not source.is_file():
                raise ValueError(f"Missing or linked package input: {relative}")
            target_path = stage / relative
            target_path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target_path)
        binary = stage / "native/runtime" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")
        binary.parent.mkdir(parents=True)
        shutil.copyfile(executable, binary)
        binary.chmod(0o755)
        wheels = stage / "wheels"
        wheels.mkdir()
        shutil.copyfile(wheel, wheels / wheel.name)
        for dependency in dependencies.iterdir():
            if dependency.is_symlink() or not dependency.is_file() or dependency.suffix != ".whl":
                raise ValueError("Dependency payload must contain regular wheels only")
            if dependency.name == wheel.name:
                raise ValueError("Duplicate application wheel")
            shutil.copyfile(dependency, wheels / dependency.name)
        files = {p.relative_to(stage).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in sorted(stage.rglob("*")) if p.is_file()}
        manifest = {"schema": 1, "version": version, "revision": revision, "platform": target,
                    "python": "3.11", "wheel": wheel.name, "files": files,
                    "minimum_os": {"linux": "Ubuntu 24.04 / glibc 2.39", "macos": "macOS 15", "windows": "Windows 10"}[target.split("-")[0]]}
        (stage / "package.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
        staged_zip = Path(temp) / "archive.zip"
        with zipfile.ZipFile(staged_zip, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(stage.rglob("*")):
                if path.is_file():
                    archive.write(path, Path(stem) / path.relative_to(stage))
        os.replace(staged_zip, destination)
    checksum = hashlib.sha256(destination.read_bytes()).hexdigest()
    destination.with_suffix(".zip.sha256").write_text(f"{checksum}  {destination.name}\n", encoding="utf-8")
    return destination


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--wheel", type=Path, required=True)
    parser.add_argument("--dependencies", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, timeout=10).decode("ascii").strip()
    print(package(ROOT, args.executable, args.wheel, args.dependencies, args.out, revision))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
