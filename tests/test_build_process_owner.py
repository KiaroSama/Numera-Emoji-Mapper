"""A successful build command must not leave a closed-stdio grandchild running."""
import ctypes
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from scripts import build_native
from tests.test_native_local_cli import ROOT

RUNS_ON_NATIVE_WINDOWS = True


class BuildProcessOwner(unittest.TestCase):
    def test_partial_member_inventory_closes_already_pinned_handles(self):
        from unittest import mock
        from scripts.build_job import Job

        job = object.__new__(Job)
        job.handle = 100
        job.kernel = mock.Mock()
        def failed_inventory(pinned):
            pinned.append(200)
            raise RuntimeError("fixture member query failed")
        job._record_members = failed_inventory
        with self.assertRaisesRegex(RuntimeError, "fixture member query failed"):
            job.finish()
        self.assertEqual(job.kernel.CloseHandle.call_args_list,
                         [mock.call(200), mock.call(100)])
        self.assertIsNone(job.handle)

    @unittest.skipUnless(os.name == "nt", "native Windows Job ownership")
    def test_success_reaps_detached_stdio_descendant(self):
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        kernel.OpenProcess.restype = ctypes.c_void_p
        kernel.WaitForSingleObject.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        kernel.CloseHandle.argtypes = [ctypes.c_void_p]
        with tempfile.TemporaryDirectory(dir=ROOT / "logs") as folder:
            ready = Path(folder) / "ready"
            child = "import pathlib,sys,threading;pathlib.Path(sys.argv[1]).write_text(str(__import__('os').getpid()),encoding='utf-8');threading.Event().wait()"
            parent = ("import pathlib,subprocess,sys,time\n"
                      "p=subprocess.Popen([sys.executable,'-c',sys.argv[1],sys.argv[2]],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)\n"
                      "deadline=time.monotonic()+5\n"
                      "while not pathlib.Path(sys.argv[2]).exists():\n"
                      " if time.monotonic()>deadline: raise RuntimeError('child not ready')\n"
                      " time.sleep(.01)\n")
            import logging
            pid = None
            try:
                with self.assertLogs("build_job", logging.INFO) as records:
                    result = build_native.run([sys.executable, "-c", parent, child, str(ready)], os.environ.copy(), 10)
                self.assertEqual(result.returncode, 0, result.stderr)
                pid = int(ready.read_text(encoding="utf-8"))
                handle = kernel.OpenProcess(0x100000, False, pid)
                if handle:
                    try:
                        self.assertEqual(kernel.WaitForSingleObject(handle, 0), 0,
                                         f"successful build left descendant {pid} alive: " + repr(records.output))
                    finally:
                        kernel.CloseHandle(handle)
            finally:
                if ready.exists() and pid is None:
                    pid = int(ready.read_text(encoding="utf-8"))
                if pid:
                    subprocess.run(["taskkill", "/F", "/T", "/PID", str(pid)],
                        capture_output=True, timeout=5, creationflags=subprocess.CREATE_NO_WINDOW)
