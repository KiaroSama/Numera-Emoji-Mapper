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
import hashlib
import tempfile

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from emojikit.logsetup import record_exit_code, setup_logging  # noqa: E402

log = logging.getLogger("build_native")


def run(command: list[str], env: dict[str, str], timeout: int) -> subprocess.CompletedProcess:
    job = None
    if os.name == "nt":
        from scripts.build_job import Job
        job = Job()
    extra = {"creationflags": subprocess.CREATE_NO_WINDOW | 4} if job else {
        "start_new_session": True}
    process = None
    try:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, **extra)
        if job:
            job.attach(process)
        out, err = process.communicate(timeout=timeout)
    finally:
        try:
            if job:
                job.finish()
            elif process:
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        finally:
            if process:
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=10)
                for stream in (process.stdout, process.stderr):
                    if stream:
                        stream.close()
    try:
        stdout, stderr = out.decode("utf-8"), err.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise RuntimeError(f"Non-UTF-8 output from {Path(command[0]).name} "
                           f"(exit {process.returncode}, byte {exc.start}); "
                           "process output cannot be safely decoded.") from exc
    return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)


def install_executable(source: Path) -> Path:
    if source.is_symlink() or not source.is_file() or not source.stat().st_size:
        raise RuntimeError("Native executable is missing or unsupported.")
    directory = ROOT / "native" / "runtime"
    directory.mkdir(parents=True, exist_ok=True)
    if directory.is_symlink() or directory.resolve().parent != (ROOT / "native").resolve():
        raise RuntimeError("Native installation directory must remain inside the project.")
    destination = directory / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")
    if destination.is_symlink():
        raise RuntimeError("Refusing to replace a linked native executable.")
    fd, name = tempfile.mkstemp(prefix=".numera-emoji-", suffix=".tmp", dir=directory)
    staged = Path(name)
    try:
        with os.fdopen(fd, "wb") as output, source.open("rb") as input_file:
            shutil.copyfileobj(input_file, output)
            output.flush()
            os.fsync(output.fileno())
        with source.open("rb") as original, staged.open("rb") as copied:
            if hashlib.file_digest(original, "sha256").digest() != hashlib.file_digest(copied, "sha256").digest():
                raise RuntimeError("Native executable copy failed byte verification.")
        shutil.copymode(source, staged)
        os.replace(staged, destination)
        return destination
    finally:
        staged.unlink(missing_ok=True)


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
    worker_limit = min(4, max(1, (os.cpu_count() or 2) - 2))
    for key in ("HOOKMAKER_MAX_TEST_WORKERS", "CARGO_BUILD_JOBS"):
        try:
            value = int(env.get(key, worker_limit))
        except ValueError:
            value = worker_limit
        if value > 0:
            worker_limit = min(worker_limit, value)
    env["CARGO_BUILD_JOBS"] = str(worker_limit)
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
                     "--locked", "--target-dir", str(ROOT / "native/target/wheel-build"),
                     "--out", str(wheels)])
    try:
        for command in commands:
            log.info("Running %s", " ".join(command))
            result = run(command, env, 600 if "maturin" in command else 180)
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
        result = run(["cargo", "build", "--manifest-path", str(ROOT / "native/Cargo.toml"),
                      "--release", "--locked", "--bin", "numera-emoji"], env, 600)
        log.info("%s", (result.stdout + result.stderr).strip())
        if result.returncode:
            log.error("Native executable build failed (exit %d).", result.returncode)
            return result.returncode
        executable = install_executable(ROOT / "native/target/release" /
                                        ("numera-emoji.exe" if os.name == "nt" else "numera-emoji"))
        probe = run([str(executable), "--help"], env, 10)
        if probe.returncode:
            log.error("Installed native executable check failed (exit %d).", probe.returncode)
            return probe.returncode
        log.info("Native executable installed and checked at %s", executable)
        log.info("Required Rust module built and imported successfully.")
        return 0
    except (OSError, subprocess.TimeoutExpired, RuntimeError) as exc:
        log.error("Native build failed: %s", exc)
        return 1


if __name__ == "__main__":
    raise SystemExit(record_exit_code(main()))
