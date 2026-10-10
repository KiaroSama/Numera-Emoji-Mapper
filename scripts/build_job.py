"""Private Windows Job ownership for bootstrap tools, independent of the Rust build."""
from __future__ import annotations

import ctypes
from ctypes import wintypes as w
import logging
import time

log = logging.getLogger("build_job")


class BasicLimit(ctypes.Structure):
    _fields_ = [("ProcessTime", ctypes.c_longlong), ("JobTime", ctypes.c_longlong),
                ("Flags", w.DWORD), ("MinWorkingSet", ctypes.c_size_t),
                ("MaxWorkingSet", ctypes.c_size_t), ("ActiveLimit", w.DWORD),
                ("Affinity", ctypes.c_size_t), ("Priority", w.DWORD), ("Scheduling", w.DWORD)]


class IOCounts(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in
                ("ReadOps", "WriteOps", "OtherOps", "ReadBytes", "WriteBytes", "OtherBytes")]


class ExtendedLimit(ctypes.Structure):
    _fields_ = [("Basic", BasicLimit), ("IO", IOCounts),
                ("ProcessMemory", ctypes.c_size_t), ("JobMemory", ctypes.c_size_t),
                ("PeakProcess", ctypes.c_size_t), ("PeakJob", ctypes.c_size_t)]


class ThreadEntry(ctypes.Structure):
    _fields_ = [("Size", w.DWORD), ("Usage", w.DWORD), ("Id", w.DWORD),
                ("Owner", w.DWORD), ("BasePriority", w.LONG), ("Delta", w.LONG), ("Flags", w.DWORD)]


class Accounting(ctypes.Structure):
    _fields_ = [("User", ctypes.c_longlong), ("Kernel", ctypes.c_longlong),
                ("PeriodUser", ctypes.c_longlong), ("PeriodKernel", ctypes.c_longlong),
                ("Faults", w.DWORD), ("Total", w.DWORD), ("Active", w.DWORD), ("Terminated", w.DWORD)]


class Job:
    def __init__(self):
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        signatures = {
            "CreateJobObjectW": ([ctypes.c_void_p, w.LPCWSTR], w.HANDLE),
            "SetInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD], w.BOOL),
            "QueryInformationJobObject": ([w.HANDLE, ctypes.c_int, ctypes.c_void_p, w.DWORD, ctypes.c_void_p], w.BOOL),
            "AssignProcessToJobObject": ([w.HANDLE, w.HANDLE], w.BOOL),
            "TerminateJobObject": ([w.HANDLE, w.UINT], w.BOOL),
            "CloseHandle": ([w.HANDLE], w.BOOL),
            "CreateToolhelp32Snapshot": ([w.DWORD, w.DWORD], w.HANDLE),
            "Thread32First": ([w.HANDLE, ctypes.POINTER(ThreadEntry)], w.BOOL),
            "Thread32Next": ([w.HANDLE, ctypes.POINTER(ThreadEntry)], w.BOOL),
            "OpenThread": ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
            "ResumeThread": ([w.HANDLE], w.DWORD),
            "OpenProcess": ([w.DWORD, w.BOOL, w.DWORD], w.HANDLE),
            "IsProcessInJob": ([w.HANDLE, w.HANDLE, ctypes.POINTER(w.BOOL)], w.BOOL),
            "WaitForSingleObject": ([w.HANDLE, w.DWORD], w.DWORD),
            "GetExitCodeProcess": ([w.HANDLE, ctypes.POINTER(w.DWORD)], w.BOOL),
            "GetProcessTimes": ([w.HANDLE] + [ctypes.POINTER(w.FILETIME)] * 4, w.BOOL),
            "QueryFullProcessImageNameW": ([w.HANDLE, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD)], w.BOOL),
        }
        for name, (args, result) in signatures.items():
            function = getattr(self.kernel, name)
            function.argtypes, function.restype = args, result
        self.handle = self.kernel.CreateJobObjectW(None, None)
        if not self.handle:
            raise ctypes.WinError(ctypes.get_last_error())
        limit = ExtendedLimit()
        limit.Basic.Flags = 0x2000  # KILL_ON_JOB_CLOSE; no breakaway.
        if not self.kernel.SetInformationJobObject(self.handle, 9, ctypes.byref(limit), ctypes.sizeof(limit)):
            self.kernel.CloseHandle(self.handle)
            self.handle = None
            raise ctypes.WinError(ctypes.get_last_error())

    def attach(self, process):
        if not self.kernel.AssignProcessToJobObject(self.handle, int(process._handle)):
            raise ctypes.WinError(ctypes.get_last_error())
        snapshot = self.kernel.CreateToolhelp32Snapshot(4, 0)
        if snapshot == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            entry = ThreadEntry()
            entry.Size = ctypes.sizeof(entry)
            found = self.kernel.Thread32First(snapshot, ctypes.byref(entry))
            while found:
                if entry.Owner == process.pid:
                    thread = self.kernel.OpenThread(2, False, entry.Id)
                    if not thread:
                        raise ctypes.WinError(ctypes.get_last_error())
                    try:
                        if self.kernel.ResumeThread(thread) != 1:
                            raise RuntimeError("Build child primary thread was not suspended once.")
                    finally:
                        self.kernel.CloseHandle(thread)
                    return
                found = self.kernel.Thread32Next(snapshot, ctypes.byref(entry))
            raise RuntimeError("Cannot find suspended build child thread.")
        finally:
            self.kernel.CloseHandle(snapshot)

    def _record_members(self, pinned):
        # Pin each reported process and recheck membership before recording it;
        # raw Job PID lists alone cannot prove that a descendant is still alive.
        slots = 8192
        buffer = ctypes.create_string_buffer(8 + slots * ctypes.sizeof(ctypes.c_size_t))
        if not self.kernel.QueryInformationJobObject(self.handle, 3, buffer, len(buffer), None):
            raise ctypes.WinError(ctypes.get_last_error())
        count = ctypes.cast(buffer, ctypes.POINTER(w.DWORD))[1]
        if count > slots:
            raise RuntimeError("Build process member inventory exceeded its bound.")
        members = (ctypes.c_size_t * count).from_buffer(buffer, 8)
        for pid in members:
            process = self.kernel.OpenProcess(0x100000 | 0x1000, False, pid)
            if not process:
                log.info("Owned Job member pid=%d could not be pinned; error=%d", pid, ctypes.get_last_error())
                continue
            try:
                member = w.BOOL()
                if not self.kernel.IsProcessInJob(process, self.handle, ctypes.byref(member)) or not member.value:
                    continue
                signal = self.kernel.WaitForSingleObject(process, 0)
                exit_code = w.DWORD()
                self.kernel.GetExitCodeProcess(process, ctypes.byref(exit_code))
                times = [w.FILETIME() for _ in range(4)]
                birth = "unknown"
                if self.kernel.GetProcessTimes(process, *(ctypes.byref(t) for t in times)):
                    birth = str((times[0].dwHighDateTime << 32) | times[0].dwLowDateTime)
                name = ctypes.create_unicode_buffer(32768)
                length = w.DWORD(len(name))
                image = "unknown"
                if self.kernel.QueryFullProcessImageNameW(process, 0, name, ctypes.byref(length)):
                    image = name.value.rsplit("\\", 1)[-1]
                log.info("Owned Job member pid=%d birth=%s image=%s signal=%d exit=%d",
                         pid, birth, image, signal, exit_code.value)
                pinned.append(process)
                process = None
            finally:
                if process:
                    self.kernel.CloseHandle(process)

    def finish(self):
        if not self.handle:
            return
        pinned = []
        try:
            self._record_members(pinned)
            if not self.kernel.TerminateJobObject(self.handle, 1):
                raise ctypes.WinError(ctypes.get_last_error())
            deadline = time.monotonic() + 10
            while True:
                accounting = Accounting()
                if not self.kernel.QueryInformationJobObject(self.handle, 1, ctypes.byref(accounting),
                                                             ctypes.sizeof(accounting), None):
                    raise ctypes.WinError(ctypes.get_last_error())
                signals = [self.kernel.WaitForSingleObject(process, 0) for process in pinned]
                if any(signal not in (0, 258) for signal in signals):
                    raise RuntimeError("Cannot verify owned build process termination.")
                if accounting.Active == 0 and all(signal == 0 for signal in signals):
                    return
                if time.monotonic() >= deadline:
                    raise RuntimeError("Build Job cleanup did not become quiescent within 10s.")
                time.sleep(.01)
        finally:
            for process in pinned:
                self.kernel.CloseHandle(process)
            self.kernel.CloseHandle(self.handle)
            self.handle = None
