"""Run the installed native command; Python here only selects the package environment."""
from __future__ import annotations

import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    sys.path.insert(0, str(ROOT))
    from scripts.local_environment import confined_directory, environment
    os.environ.update(environment(ROOT))
    confined_directory(ROOT, ".venv", create=False)
    confined_directory(ROOT, ".venv/Scripts" if os.name == "nt" else ".venv/bin", create=False)
    confined_directory(ROOT, "native/runtime", create=False)
    executable = ROOT / "native/runtime" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")
    python = ROOT / (".venv/Scripts/python.exe" if os.name == "nt" else ".venv/bin/python")
    if not executable.is_file() or not python.is_file():
        print("Prebuilt runtime is not installed; run scripts/install_prebuilt.py first.", file=sys.stderr)
        return 2
    if Path(sys.prefix).resolve() != (ROOT / ".venv").resolve():
        os.execv(str(python), [str(python), str(Path(__file__).resolve()), *sys.argv[1:]])
    if os.name != "nt":
        executable.chmod(0o755)
    # Bind even hermetic native commands to this package, never an inherited build interpreter.
    os.environ["PYO3_PYTHON"] = str(python)
    os.chdir(ROOT)
    os.execv(str(executable), [str(executable), *sys.argv[1:]])
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
