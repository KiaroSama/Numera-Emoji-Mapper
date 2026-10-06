"""Build the required Rust extension for the selected project interpreter."""

from __future__ import annotations

import importlib.util
import logging
import os
from pathlib import Path
import shutil
import subprocess
import sys
import sysconfig
import signal

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from emojikit.logsetup import record_exit_code, setup_logging  # noqa: E402

log = logging.getLogger("build_native")


def run(command: list[str], env: dict[str, str], timeout: int) -> subprocess.CompletedProcess:
    extra = {"creationflags": subprocess.CREATE_NO_WINDOW} if os.name == "nt" else {
        "start_new_session": True}
    process = subprocess.Popen(command, cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, **extra)
    try:
        out, err = process.communicate(timeout=timeout)
    except BaseException:
        if os.name == "nt":
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(process.pid)],
                           capture_output=True, timeout=10, check=False)
        else:
            os.killpg(process.pid, signal.SIGKILL)
        process.kill()
        process.communicate(timeout=10)
        raise
    return subprocess.CompletedProcess(command, process.returncode,
                                       out.decode("utf-8"), err.decode("utf-8"))


def main() -> int:
    setup_logging("build_native")
    if shutil.which("cargo") is None:
        log.error("Rust 1.99+ and the platform linker are required. Install Rust, "
                  "then run: python scripts/build_native.py")
        return 2
    env = os.environ.copy()
    env["PYTHONUTF8"] = "1"
    env["PYO3_PYTHON"] = sys.executable
    env["PATH"] = sysconfig.get_path("scripts") + os.pathsep + env.get("PATH", "")
    env["CARGO_BUILD_JOBS"] = str(min(4, max(1, (os.cpu_count() or 2) - 2)))
    temp = ROOT / "logs" / "native-build"
    temp.mkdir(parents=True, exist_ok=True)
    for key in ("TEMP", "TMP", "TMPDIR"):
        env[key] = str(temp)
    # Build dependencies belong to this selected environment, never --user.
    commands = []
    if importlib.util.find_spec("maturin") is None:
        commands.append([sys.executable, "-m", "pip", "install", "maturin==1.15.0"])
    wheels = ROOT / "native" / "target" / "wheels"
    commands.append([sys.executable, "-m", "maturin", "build", "--release",
                     "--locked", "--out", str(wheels)])
    try:
        for command in commands:
            log.info("Running %s", " ".join(command))
            result = run(command, env, 180)
            if result.stdout:
                log.info("%s", result.stdout.strip())
            if result.stderr:
                log.info("%s", result.stderr.strip())
            if result.returncode:
                log.error("Native build failed (exit %d).", result.returncode)
                return result.returncode
        built = list(wheels.glob("numera_emoji_core-2.0.0-*.whl"))
        if len(built) != 1:
            log.error("Expected one wheel for this platform; clean native/target/wheels and rebuild.")
            return 1
        result = run([sys.executable, "-m", "pip", "install", "--no-deps",
                      "--force-reinstall", str(built[0])], env, 60)
        log.info("%s", (result.stdout + result.stderr).strip())
        if result.returncode:
            return result.returncode
        # The source checkout shadows the installed package, so use its binary.
        from importlib.machinery import EXTENSION_SUFFIXES
        package = Path(sysconfig.get_path("platlib")) / "emojikit"
        matches = [p for suffix in EXTENSION_SUFFIXES for p in package.glob("_native*" + suffix)]
        if not matches:
            log.error("Installed wheel has no native module.")
            return 1
        source = matches[0]
        destination = ROOT / "emojikit" / source.name
        staged = destination.with_suffix(destination.suffix + ".tmp")
        try:
            shutil.copy2(source, staged)
            staged.replace(destination)
        finally:
            staged.unlink(missing_ok=True)
        from emojikit.similarity import require_native
        require_native()
        log.info("Required Rust module built and imported successfully.")
        return 0
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as exc:
        log.error("Native build failed: %s", exc)
        return 1


if __name__ == "__main__":
    raise SystemExit(record_exit_code(main()))
