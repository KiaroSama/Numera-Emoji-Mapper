"""Read Darwin PID birth identity using the same libproc ABI as native ownership."""
from __future__ import annotations

import ctypes


class _BsdInfo(ctypes.Structure):
    _fields_ = [("header", ctypes.c_uint32 * 12), ("comm", ctypes.c_char * 16),
                ("name", ctypes.c_char * 32), ("tail", ctypes.c_uint32 * 6),
                ("birth_sec", ctypes.c_uint64), ("birth_usec", ctypes.c_uint64)]


def birth(pid: int) -> int:
    if pid <= 0 or ctypes.sizeof(_BsdInfo) != 136:
        raise RuntimeError("Invalid Darwin process identity request")
    library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    library.proc_pidinfo.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint64,
                                    ctypes.c_void_p, ctypes.c_int]
    library.proc_pidinfo.restype = ctypes.c_int
    info = _BsdInfo()
    size = ctypes.sizeof(info)
    if library.proc_pidinfo(pid, 3, 1, ctypes.byref(info), size) != size:
        raise RuntimeError("Cannot inspect Darwin parent birth identity")
    if info.header[3] != pid or not 1 <= info.header[1] < 5 or not info.birth_sec or info.birth_usec >= 1_000_000:
        raise RuntimeError("Invalid Darwin parent birth identity")
    return info.birth_sec * 1_000_000 + info.birth_usec
