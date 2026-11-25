"""Exercise the full library, remote paging and new native sources on Windows."""

import ctypes as c
from ctypes import wintypes as w
import json
from pathlib import Path
import sys
import time

from PIL import ImageChops, ImageGrab, ImageStat

import run

sys.path.insert(0, str(run.ROOT))
from verify import Input, Mouse, USER32 as u, click_button, click_client


class Memory(c.Structure):
    _fields_ = [('cb', w.DWORD), ('faults', w.DWORD), ('sizes', c.c_size_t * 9),
                ('private_ws', c.c_size_t), ('shared_commit', c.c_uint64)]


def memory(pid):
    query = run.K.K32GetProcessMemoryInfo
    query.argtypes = [w.HANDLE, c.POINTER(Memory), w.DWORD]
    query.restype = w.BOOL
    h = run.K.OpenProcess(0x1000, False, pid)
    try:
        m = Memory()
        m.cb = c.sizeof(m)
        assert h and query(h, c.byref(m), m.cb)
        return m.private_ws / 1048576
    finally:
        if h:
            run.K.CloseHandle(h)


class Check(run.Playlist):
    def tick(self):
        self.check()
        raise SystemExit(0)

    def until(self, condition, timeout=25):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            super().tick()
            assert self.viewer.poll() is None and self.worker.process.poll() is None
            if result := condition():
                return result
            time.sleep(.06)
        raise TimeoutError('Catalog interaction timed out')

    def check(self):
        log = self.folder / 'viewer.log'
        self.until(lambda: 'Startup working set trim: done' in log.read_text(encoding='utf-8'), 45)
        hwnd, = run.windows_on_display(self.viewer.pid)
        rect, origin, cursor = w.RECT(), w.POINT(), w.POINT()
        assert u.GetClientRect(hwnd, c.byref(rect))
        assert u.ClientToScreen(hwnd, c.byref(origin)) and u.GetCursorPos(c.byref(cursor))
        assert u.IsZoomed(hwnd)
        dpi = u.GetDpiForWindow(hwnd) / 96
        dock_scale = min(dpi, rect.right / 2160, rect.bottom / 560)
        limit = rect.bottom - 190 * dock_scale
        scale = min(dpi, rect.right / 1326, limit / 540)
        height = min(rect.bottom * .92 / scale, 900, limit / scale)
        x = (rect.right - 1260 * scale) / 2
        y = min((rect.bottom - height * scale) / 2, limit - height * scale)
        results = {'library': len(self.items), 'display': [rect.right, rect.bottom]}

        def click(xx, yy):
            click_client(hwnd, round(x + xx * scale), round(y + yy * scale))

        def snapshot(name):
            ImageGrab.grab((origin.x + round(x), origin.y + round(y), origin.x + round(x + 1260 * scale),
                            origin.y + round(y + height * scale)), all_screens=True).save(self.folder / (name + '.png'))

        def check_scroll_images():
            # This poster stays visible across a whole row boundary. Check the
            # actual pixels before and after the wheel event, including frames
            # shown while the thumbnail loader catches up.
            left = origin.x + round(x + (20 + 245 + 40) * scale)
            original = origin.y + round(y + (206 + 2 * 173 + 42) * scale)
            pixels = round(210 * dpi)  # Five wheel notches, 42 units each.
            width, band = round(90 * scale), round(25 * scale)
            before = pixels + 1
            region = (left, original - before, left + width, original + band + 1)

            def grab():
                return ImageGrab.grab(region, all_screens=True)

            def crop(frame, at):
                return frame.crop((0, at, width, at + band))

            def distance(frame, at):
                # Account for rounding to physical pixels with fractional DPI.
                return min(max(ImageStat.Stat(ImageChops.difference(reference, crop(frame, at + d))).mean)
                           for d in (-1, 0, 1))

            def scroll(data):
                event = Input(0, Mouse(0, 0, data & 0xffffffff, 0x0800, 0, 0))
                assert u.SendInput(1, c.byref(event), c.sizeof(event)) == 1

            # Leave the cursor away from the observed cards so hover is constant.
            assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
            assert u.SetCursorPos(origin.x + round(x + 1255 * scale),
                                  origin.y + round(y + 300 * scale))
            reference = crop(grab(), before)
            assert max(ImageStat.Stat(reference).stddev) > 15, 'Need a visible poster to check for flicker'
            samples = 0
            ram = 0
            for data, target in ((-600, 1), (600, before)) * 3:
                scroll(data)
                end = time.monotonic() + .25
                moved = False
                while time.monotonic() < end:
                    frame = grab()
                    at_target = distance(frame, target) < 6
                    other = before if target == 1 else 1
                    if not (at_target or distance(frame, other) < 6):
                        frame.save(self.folder / 'catalog-scroll-flicker.png')
                        raise AssertionError(f'A visible poster disappeared or changed during scroll ({results["open_mib"]} MiB at rest)')
                    moved |= at_target
                    samples += 1
                    ram = max(ram, memory(self.viewer.pid))
                assert moved, 'The wheel event did not scroll the catalog'
                super(Check, self).tick()
            return {'frames_sampled': samples, 'peak_private_ws_mib': round(ram, 2)}

        def thumb():
            sx = origin.x + round(x + 1248 * scale)
            sy = origin.y + round(y)
            strip = ImageGrab.grab((sx, sy, sx + 1, origin.y + round(y + height * scale)),
                                   all_screens=True)
            pixels = [i for i in range(round(216 * scale), round((height - 50) * scale))
                      if strip.getpixel((0, i)) in ((75, 21, 242), (88, 32, 255))]
            if pixels:
                return [(i + .5) / scale for i in (pixels[0], pixels[-1])]

        def drag_thumb(start, end):
            sx = origin.x + round(x + 1248 * scale)
            def move(yy):
                assert u.SetCursorPos(sx, origin.y + round(y + yy * scale))
                time.sleep(.12)

            def button(flags):
                event = Input(0, Mouse(0, 0, 0, flags, 0, 0))
                assert u.SendInput(1, c.byref(event), c.sizeof(event)) == 1
                time.sleep(.12)

            assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
            move(start)
            button(2)
            try:
                move(end)
            finally:
                button(4)

        def sample():
            vals = []
            for _ in range(12):
                super(Check, self).tick()
                vals.append(memory(self.viewer.pid))
                time.sleep(.15)
            return [round(min(vals), 2), round(max(vals), 2)]

        def online_after(serial):
            def read():
                path = self.folder / 'online.json'
                if path.is_file():
                    d = json.loads(path.read_text(encoding='utf-8'))
                    if d['serial'] > serial:
                        assert not d.get('error'), d
                        assert d['rows']
                        time.sleep(.35)  # Wait for the native panel to consume the reply.
                        return d
            return self.until(read)

        def posters_ready(rows):
            keys = [r['poster'] for r in rows if r['poster']]
            return lambda: sum((run.catalog.CACHE / run.catalog.POSTERS / (key + '.png')).is_file()
                               for key in keys) >= min(18, len(keys))

        def source(kind, name=None):
            previous = self.worker.process
            self.until(lambda: self.items[self.index].kind == kind and
                       (not name or self.items[self.index].name == name) and
                       self.items[self.index].title + ' | blur=' in log.read_text(encoding='utf-8'))
            assert previous.poll() is not None
            assert not run.windows_on_display(self.worker.process.pid, self.worker.pid)
            surf = (self.folder / f'surface-{self.generation}.txt').read_text().split()
            return [int(surf[1]), int(surf[2])]

        try:
            assert self.folder.joinpath('library.json').is_file() and len(self.items) > 3
            assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
            results['closed_before_mib'] = sample()
            click_button(hwnd, 'wallpapers')
            library = json.loads(self.folder.joinpath('library.json').read_text(encoding='utf-8'))
            self.until(posters_ready(library[:20]))
            self.until(posters_ready([library[11]]))
            time.sleep(.7)
            results['open_mib'] = sample()
            snapshot('catalog-downloaded')
            results['wheel_posters'] = check_scroll_images()
            top = self.until(thumb)
            assert abs(top[0] - 216) < 3, top
            click(1248, top[1] + 60)
            paged = self.until(lambda: (t := thumb()) and t[0] > top[0] + 20 and t)
            drag_thumb(sum(paged) / 2, height - 12)
            bottom = self.until(lambda: (t := thumb()) and abs(t[1] - (height - 50)) < 3 and t)
            drag_thumb(sum(bottom) / 2, 170)
            self.until(lambda: (t := thumb()) and abs(t[0] - top[0]) < 3)
            results['scrollbar'] = {'top': top, 'after_track_click': paged, 'bottom': bottom}
            click(410, 179)
            self.until(posters_ready([r for r in library if r.get('source') == 'motion'][:20]))
            time.sleep(.35)
            snapshot('catalog-motion-downloaded')
            click(450, 83)
            first = online_after(0)
            self.until(posters_ready(first['rows'][:20]))
            time.sleep(.7)
            snapshot('catalog-motion')
            assert first['more']
            click(1208, height - 21)
            second = online_after(first['serial'])
            assert second['rows'][0]['id'] != first['rows'][0]['id']
            snapshot('catalog-motion-page-2')
            # Apply an item already downloaded in the old launcher (no duplicate file).
            idx = next(i for i, r in enumerate(second['rows'][:5]) if r['slug'] == 'minecraft-aquarium')
            click(120 + 245 * idx, 270)
            results['motion_native_px'] = source('video', 'Minecraft Aquarium')
            assert results['motion_native_px'] == [3840, 2160]
            click(250, 83)
            click(880, 132)
            steam = online_after(second['serial'])
            self.until(posters_ready(steam['rows'][:20]))
            time.sleep(.7)
            snapshot('catalog-steam')
            click(1208, height - 21)
            steam2 = online_after(steam['serial'])
            assert steam2['rows'][0]['id'] != steam['rows'][0]['id']
            click(80, 83)
            click(560, 179)
            # Imported files are in their own category; laika2 is the first.
            self.until(posters_ready([r for r in library if r.get('source') == 'imported']))
            time.sleep(.3)
            assert thumb() is None
            click(110, 270)
            results['imported_image_px'] = source('image', 'laika2')
            snapshot('catalog-imported')
            click(1224, 31)
            time.sleep(.7)
            results['closed_after_mib'] = sample()
            (self.folder / 'catalog-proof.json').write_text(json.dumps(results, indent=2))
            print(json.dumps(results, indent=2), flush=True)
        finally:
            u.SetWindowPos(hwnd, -2, 0, 0, 0, 0, 0x13)
            u.SetCursorPos(cursor.x, cursor.y)


if __name__ == '__main__':
    run.Playlist = Check
    sys.argv.append('--no-save')
    run.main()
