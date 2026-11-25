"""Check the native loading cover and reveal while withholding the first source."""
import ctypes as c
from ctypes import wintypes as w
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from PIL import Image, ImageGrab

import run


def main():
    exe = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else run.VIEWER
    out = Path(tempfile.mkdtemp(prefix='native-startup-', dir=run.ROOT / 'scratch'))
    print(out, flush=True)
    rgb = (68, 121, 193)
    source = out / 'source.png'
    Image.new('RGB', (640, 360), rgb).save(source)
    u = run.U
    u.SetProcessDpiAwarenessContext.argtypes = [c.c_void_p]
    u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
    u.ClientToScreen.argtypes = [w.HWND, c.POINTER(w.POINT)]
    u.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
    worker = None
    with (out / 'viewer.log').open('w', encoding='utf-8') as log:
        viewer = subprocess.Popen([exe, '--await-source', out / 'surface-0.txt'], stdout=log, stderr=log,
                                  creationflags=subprocess.CREATE_NO_WINDOW)
    try:
        hwnd, = run.wait_for(lambda: run.windows_on_display(viewer.pid), viewer, timeout=15)
        bounds, origin = w.RECT(), w.POINT()
        assert u.GetClientRect(hwnd, c.byref(bounds)) and u.ClientToScreen(hwnd, c.byref(origin))
        assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
        width, height = bounds.right, bounds.bottom
        box = (origin.x, origin.y, origin.x + width, origin.y + height)
        center = (width // 2 - 100, height // 2 - 100, width // 2 + 100, height // 2 + 100)
        time.sleep(0.08)
        loading = ImageGrab.grab(bbox=box, all_screens=True).convert('RGB')
        loading.save(out / 'loading.png')
        assert loading.getpixel((40, 40)) == (4, 4, 22)
        assert sum(min(pixel) > 145 for pixel in loading.crop(center).getdata()) > 40

        worker = run.Worker(run.Media.from_path(source), 0, 0, out, out, run.BIN, viewer, False, out, 60)
        # Sample clear wallpaper above Home's carousel.
        probes = [(width * x // 10, height // 2) for x in (2, 4, 6, 8)]
        first_seen = [None] * len(probes)
        start = time.monotonic()
        saved_reveal = False
        while time.monotonic() - start < 2.5:
            image = ImageGrab.grab(bbox=box, all_screens=True).convert('RGB')
            ms = round(1000 * (time.monotonic() - start))
            for i, probe in enumerate(probes):
                if first_seen[i] is None and image.getpixel(probe) == rgb:
                    first_seen[i] = ms
            if all(t is not None for t in first_seen):
                image.save(out / 'revealed.png')
                break
            if not saved_reveal and first_seen[0] is not None:
                image.save(out / 'reveal.png')
                saved_reveal = True
        assert all(t is not None for t in first_seen), first_seen
        assert first_seen == sorted(first_seen) and 0 < first_seen[-1] - first_seen[0], first_seen
        assert run.windows_on_display(viewer.pid) == [hwnd]
        print(f'Loading cube, reveal from left to right, and same window: {first_seen} ms')
    finally:
        run.stop(viewer, close_window=True)
        if worker:
            worker.close()


if __name__ == '__main__':
    main()
