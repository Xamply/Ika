"""Wake the Windows bridge without periodically probing the control files."""
import ctypes as c
from ctypes import wintypes as w

_kernel = c.WinDLL('kernel32', use_last_error=True)
_kernel.CreateEventW.argtypes = [c.c_void_p, w.BOOL, w.BOOL, w.LPCWSTR]
_kernel.CreateEventW.restype = w.HANDLE
_kernel.SetEvent.argtypes = [w.HANDLE]
_kernel.CloseHandle.argtypes = [w.HANDLE]
_kernel.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
_kernel.WaitForSingleObject.restype = w.DWORD


class Event:
    def __init__(self, folder, suffix, *, manual=False):
        self.name = f'Local\\IkaWallpaper-{folder.name}-{suffix}'
        self.handle = _kernel.CreateEventW(None, manual, False, self.name)
        if not self.handle:
            raise c.WinError(c.get_last_error())

    def set(self):
        if not _kernel.SetEvent(self.handle):
            raise c.WinError(c.get_last_error())

    def wait(self, seconds):
        return _kernel.WaitForSingleObject(self.handle, round(seconds * 1000)) == 0

    def close(self):
        if self.handle:
            _kernel.CloseHandle(self.handle)
            self.handle = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
