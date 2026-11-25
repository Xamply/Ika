"""Keep lossless first frames across launcher sessions."""

import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import struct

import catalog

VIDEO_PART = 'video-mpv.exe'


HEADER = struct.Struct('<4sIIII')
LIMIT = 3 * 1024**3


def stamp(path):
    try:
        info = path.stat()
        return str(path), info.st_mtime_ns, info.st_size
    except (OSError, ValueError):
        return str(path), None, None


def primary_size():
    u = c.WinDLL('user32')
    gdi = c.WinDLL('gdi32')
    u.GetDC.argtypes = [w.HWND]
    u.GetDC.restype = w.HDC
    u.ReleaseDC.argtypes = [w.HWND, w.HDC]
    gdi.GetDeviceCaps.argtypes = [w.HDC, c.c_int]
    dc = u.GetDC(None)
    if not dc:
        return 0, 0
    try:
        return gdi.GetDeviceCaps(dc, 118), gdi.GetDeviceCaps(dc, 117)
    finally:
        u.ReleaseDC(None, dc)


class FrameCache:
    def __init__(self, root, engine, binaries, scene_720p, fps):
        self.root = root
        self.root.mkdir(parents=True, exist_ok=True)
        self.scene = [1, stamp(engine / 'wallpaper64.exe'), stamp(engine / 'version.json'),
                      stamp(binaries / 'producer.dll'), primary_size(), scene_720p, fps]
        video = binaries / (VIDEO_PART if (binaries / VIDEO_PART).is_file() else 'video.exe')
        self.media = [1, stamp(video), stamp(binaries / 'libmpv-2.dll')]

    def path(self, item, settings=None):
        if item.scene:
            shaders = item.path / 'shaders/blobsSM40'
            assets = [stamp(item.package), stamp(item.path / 'project.json'), stamp(shaders)]
            # A Workshop preset can refer to its own image or other local file.
            assets.extend(stamp(Path(value)) for value in (item.preset or {}).values()
                          if isinstance(value, str) and Path(value).is_absolute())
            data = [self.scene, assets, item.preset, settings]
        else:
            data = [self.media, stamp(item.path)]
        key = catalog.key(json.dumps([item.id, item.kind, data], sort_keys=True))
        return self.root / f'{key}.ikf'

    @staticmethod
    def ready(path):
        if path is None:
            return False
        try:
            with path.open('rb') as file:
                magic, width, height, fmt, payload = HEADER.unpack(file.read(HEADER.size))
                return (magic == b'IKF1' and 0 < width <= 8192 and 0 < height <= 8192
                        and fmt in (28, 87) and 0 < payload <= width * height * 4
                        and os.fstat(file.fileno()).st_size == HEADER.size + payload)
        except (OSError, struct.error):
            return False

    def trim(self, keep):
        try:
            files = [(path, path.stat()) for path in self.root.glob('*.ikf')]
        except OSError:
            return
        size = sum(info.st_size for _, info in files)
        for path, info in sorted(files, key=lambda entry: (entry[0] in keep, entry[1].st_mtime_ns)):
            if size <= LIMIT:
                break
            try:
                path.unlink()
                size -= info.st_size
            except OSError:
                pass
