"""Install bundled binary wheels; never invoke Rust, Cargo or a source build."""
from __future__ import annotations

import argparse
from importlib.machinery import EXTENSION_SUFFIXES
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from emojikit.logsetup import record_exit_code, setup_logging  # noqa: E402
from scripts.prebuilt_manifest import verify  # noqa: E402
from scripts.local_environment import confined_directory, environment as local_environment  # noqa: E402
from scripts import build_native  # noqa: E402


def install() -> int:
    manifest = verify(ROOT)
    if (ROOT / ".venv").exists() or (ROOT / ".venv").is_symlink():
        raise ValueError("Existing installation preserved; extract the new package to a separate directory")
    environment = local_environment(ROOT)
    log = setup_logging("install_prebuilt")
    os.environ.update(environment)
    tempfile.tempdir = environment["TMPDIR"]
    python = ROOT / (".venv/Scripts/python.exe" if os.name == "nt" else ".venv/bin/python")
    confined_directory(ROOT, ".venv", create=False)
    confined_directory(ROOT, ".venv/Scripts" if os.name == "nt" else ".venv/bin", create=False)
    if python.is_symlink() and os.name == "nt":
        raise ValueError("Linked Windows interpreter is not supported")
    if not python.exists():
        result = build_native.run([sys.executable, "-m", "venv", str(ROOT / ".venv")], environment, 60)
        if result.returncode:
            log.error("Local virtual environment creation failed: %s", result.stderr)
            return result.returncode
    prefix = build_native.run([str(python), "-c", "import sys; print(sys.prefix)"], environment, 10)
    if prefix.returncode or Path(prefix.stdout.strip()).resolve() != (ROOT / ".venv").resolve():
        raise ValueError("Interpreter does not belong to this package virtual environment")
    command = [str(python), "-m", "pip", "install", "--no-index", "--only-binary=:all:",
               "--find-links", str(ROOT / "wheels"), "-r", str(ROOT / "requirements.txt"),
               "-r", str(ROOT / "requirements-coins.txt"), str(ROOT / "wheels" / manifest["wheel"])]
    result = build_native.run(command, environment, 180)
    if result.returncode:
        log.error("Binary dependency installation failed: %s", result.stderr)
        return result.returncode
    probe = build_native.run([str(python), "-c", "import sysconfig; print(sysconfig.get_path('platlib'))"], environment, 10)
    if probe.returncode:
        raise RuntimeError("Cannot resolve installed native package")
    package = Path(probe.stdout.strip()) / "emojikit"
    matches = sorted({path for suffix in EXTENSION_SUFFIXES for path in package.glob("_native*" + suffix)})
    if len(matches) != 1:
        raise RuntimeError("Installed package must contain exactly one native extension")
    build_native.install_extension(matches[0])
    executable = ROOT / "native/runtime" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")
    if os.name != "nt":
        executable.chmod(0o755)
    help_result = build_native.run([str(executable), "--help"], environment, 10)
    if help_result.returncode:
        raise RuntimeError("Installed native executable did not start")
    log.info("Prebuilt %s installed; no Rust/Cargo compilation was performed", manifest["platform"])
    print("Installed. Run the application with the package's Python launcher:")
    print("  .venv/Scripts/python.exe scripts/launch_prebuilt.py --help" if os.name == "nt"
          else "  .venv/bin/python scripts/launch_prebuilt.py --help")
    return 0


def main() -> int:
    argparse.ArgumentParser(description=__doc__).parse_args()
    try:
        return record_exit_code(install())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Installation refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
