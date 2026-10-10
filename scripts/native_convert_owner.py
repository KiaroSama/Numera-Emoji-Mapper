"""Own one coin-watchdog native conversion attempt, including closed-stdio descendants."""
from __future__ import annotations

import argparse
import logging
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
from emojikit.logsetup import record_exit_code, setup_logging  # noqa: E402

log = logging.getLogger("native_convert_owner")


def marker_state(marker: Path):
    try:
        return marker.stat().st_mtime_ns, marker.read_bytes()
    except FileNotFoundError:
        return None


def run(source: Path, output: Path, stdout: Path, stderr: Path) -> int:
    executable = ROOT / "native/runtime" / ("numera-emoji.exe" if os.name == "nt" else "numera-emoji")
    if not executable.is_file():
        raise RuntimeError("Installed native converter missing; run scripts/build_native.py first.")
    job = None
    if os.name == "nt":
        from scripts.build_job import Job
        job = Job()
    child = None
    try:
        marker = output / ".svg_cur"
        state = marker_state(marker)
        started = progressed = time.monotonic()
        with stdout.open("wb") as out, stderr.open("wb") as err:
            child = subprocess.Popen([str(executable), "make-emoji-pngs", "--in", str(source), "--out", str(output)],
                cwd=ROOT, env={**os.environ, "PYO3_PYTHON": sys.executable}, stdin=subprocess.DEVNULL,
                stdout=out, stderr=err, creationflags=(subprocess.CREATE_NO_WINDOW | 4) if job else 0,
                start_new_session=not job)
            if job:
                job.attach(child)
            while True:
                try:
                    code = child.wait(timeout=2)
                    log.info("Native conversion attempt exited with code %d", code)
                    return code
                except subprocess.TimeoutExpired:
                    now = time.monotonic()
                    current = marker_state(marker)
                    if current != state:
                        state, progressed = current, now
                    if now - started >= 3600:
                        log.error("Native conversion exceeded the 3600s wall bound")
                        return 125
                    if now - progressed >= 120:
                        log.error("Native conversion marker made no progress for 120s")
                        return 124
    finally:
        try:
            if job:
                job.finish()
            elif child:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        except (OSError, RuntimeError) as exc:
            log.error("Owned conversion tree cleanup failed: %s", exc)
            raise
        finally:
            if child:
                if child.poll() is None:
                    child.kill()
                try:
                    child.wait(timeout=10)
                except (OSError, subprocess.SubprocessError) as exc:
                    log.error("Owned conversion child did not finalize: %s", exc)
                    raise


def main() -> int:
    setup_logging("native_convert_owner")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--in", dest="source", type=Path, required=True)
    parser.add_argument("--out", dest="output", type=Path, required=True)
    parser.add_argument("--stdout", type=Path, required=True)
    parser.add_argument("--stderr", type=Path, required=True)
    args = parser.parse_args()
    try:
        return record_exit_code(run(args.source, args.output, args.stdout, args.stderr))
    except (OSError, RuntimeError, subprocess.SubprocessError) as exc:
        log.error("Native converter ownership failed: %s", exc)
        return record_exit_code(1)


if __name__ == "__main__":
    raise SystemExit(main())
