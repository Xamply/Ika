"""Exercise the native Wallpapers window and capture the Play hover on Windows."""

import ctypes as c
from ctypes import wintypes as w
import json
import sys
import time

from PIL import ImageChops, ImageGrab, ImageStat

import run

sys.path.insert(0, str(run.ROOT))
from verify import USER32 as u, Input, Mouse, click_button, click_client


class Key(c.Structure):
    _fields_ = [('vk', w.WORD), ('scan', w.WORD), ('flags', w.DWORD),
                ('time', w.DWORD), ('extra', c.c_size_t)]


class Check(run.Playlist):
    def tick(self):
        self.check()
        raise SystemExit(0)

    def check(self):
        log = self.folder / 'viewer.log'

        def launched():
            if 'Startup working set trim: done' in log.read_text(encoding='utf-8'):
                return run.windows_on_display(self.viewer.pid)

        hwnd, = run.wait_for(launched, self.worker.process, self.viewer, timeout=40)
        bounds, origin, previous = w.RECT(), w.POINT(), w.POINT()
        assert u.GetClientRect(hwnd, c.byref(bounds))
        assert u.ClientToScreen(hwnd, c.byref(origin)) and u.GetCursorPos(c.byref(previous))
        dpi = u.GetDpiForWindow(hwnd) / 96
        width, height = bounds.right, bounds.bottom
        dock_scale = min(dpi, width / 2160, height / 560)
        limit = height - 190 * dock_scale
        scale = min(dpi, width / 1326, limit / 540)
        panel_h = min(height * .92 / scale, 900, limit / scale)
        panel_x = (width - 1260 * scale) / 2
        panel_y = min((height - panel_h * scale) / 2, limit - panel_h * scale)
        assert u.IsZoomed(hwnd)

        def place_on_top():
            assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)

        def grab(region):
            place_on_top()
            l, t, r, b = region
            return ImageGrab.grab((origin.x + l, origin.y + t, origin.x + r, origin.y + b), all_screens=True)

        def click(x, y):
            click_client(hwnd, round(panel_x + x * scale), round(panel_y + y * scale))

        def snapshot(name):
            image = grab((round(panel_x), round(panel_y), round(panel_x + 1260 * scale), round(panel_y + panel_h * scale)))
            image.save(self.folder / f'{name}.png')
            return image

        def keys(*codes):
            u.GetForegroundWindow.restype = w.HWND
            assert u.GetForegroundWindow() == hwnd
            batch = (Input * (len(codes) * 2))()
            for i, vk in enumerate(codes):
                for step, flags in enumerate((0, 2)):
                    entry = batch[2 * i + step]
                    entry.type = 1
                    key = Key(vk, 0, flags, 0, 0)
                    assert c.sizeof(Key) <= c.sizeof(Mouse)
                    c.memmove(c.addressof(entry.mouse), c.addressof(key), c.sizeof(Key))
            assert u.SendInput(len(batch), batch, c.sizeof(Input)) == len(batch)

        def switch_to(index, timeout=12):
            old = self.worker.process
            path = self.folder / f'request-{self.generation+1}'
            run.wait_for(path.exists, self.worker.process, self.viewer, timeout)
            assert int(path.read_text(encoding='ascii')) == index
            super(Check, self).tick()
            assert self.index == index and old.poll() is not None
            run.wait_for(lambda: self.items[index].title in log.read_text(encoding='utf-8'),
                         self.worker.process, self.viewer)

        try:
            place_on_top()
            y = round(height - 139 * dock_scale)
            x = width // 2
            region = (x - round(194 * dock_scale), y - round(46 * dock_scale),
                      x + round(194 * dock_scale), y + round(46 * dock_scale))
            assert u.SetCursorPos(origin.x + x, origin.y + y - 160)
            time.sleep(.18)
            rest = grab(region)
            assert u.SetCursorPos(origin.x + x, origin.y + y)
            time.sleep(.42)
            hover = grab(region)
            time.sleep(.29)
            later = grab(region)
            inner = (round(38 * dock_scale), round(18 * dock_scale),
                     round(350 * dock_scale), round(74 * dock_scale))
            assert ImageStat.Stat(ImageChops.difference(rest.crop(inner), hover.crop(inner))).mean[0] > 2.0
            assert ImageStat.Stat(ImageChops.difference(hover.crop(inner), later.crop(inner))).mean[0] > .05
            rest.save(self.folder / 'play-rest.png')
            hover.save(self.folder / 'play-hover.png')
            later.save(self.folder / 'play-orbit.png')

            click_button(hwnd, 'wallpapers')
            time.sleep(1)
            settled = snapshot('wallpapers')
            assert settled.getpixel((round(700 * scale), round(400 * scale))) == (4, 4, 22)
            assert not (self.folder / 'request-1').exists(), 'Opening switched the background'

            # Select item 3 directly: the old Next-only control would select item 2.
            click(620, 245)
            switch_to(2)
            time.sleep(.18)
            snapshot('wallpapers-applied')

            click(230, 83)  # Wallpaper Engine: first two cards, no local MP4
            time.sleep(.48)
            local = snapshot('wallpapers-scenes')
            assert local.getpixel((round(620 * scale), round(245 * scale))) == (4, 4, 22)
            click(220, 134)
            keys(ord('L'), ord('Y'), ord('N'))
            time.sleep(.25)
            search = snapshot('wallpapers-search')
            assert search.getpixel((round(370 * scale), round(245 * scale))) == (4, 4, 22)

            click(560, 83)
            click(500, 134)  # Add currently playing MP4 to the playlist
            click(230, 83)
            click(375, 245)  # Switching tabs cleared the filter; Lynae is second
            switch_to(1)
            click(560, 83)
            click(500, 134)  # Add currently playing scene to the playlist
            time.sleep(.45)
            snapshot('wallpapers-playlist')
            for _ in range(5):  # 60 → 180 → 300 → 600 → 1800 → 15 s
                click(360, 132)
            click(100, 132)
            switch_to(2, timeout=20)  # Rotates to the other entry in the list.
            click(100, 132)
            click(120, 245)
            switch_to(1)

            click(203, 269)  # Scene properties.
            props_file = self.folder / 'properties.json'
            end = time.monotonic() + 5
            while not props_file.is_file() and time.monotonic() < end:
                self.catalog.tick()
                time.sleep(.06)
            controls = json.loads(props_file.read_text(encoding='utf-8'))
            assert controls['index'] == 1 and controls['rows'][0]['type'] == 'bool'
            name = controls['rows'][0]['key']
            default = controls['rows'][0]['value']
            time.sleep(.3)
            click(1204, 279)
            snapshot('wallpapers-properties')
            click(1175, panel_h - 66)
            end = time.monotonic() + 5
            request = self.folder / f'request-{self.generation + 1}'
            while not request.is_file() and time.monotonic() < end:
                self.catalog.tick()
                time.sleep(.06)
            switch_to(1)
            config = json.loads((self.worker.runtime / 'config.json').read_text(encoding='utf-8'))
            props = next(iter(next(iter(config.values()))['wproperties'].values()))
            assert props['Monitor0'][name] == (not default)
            click(1213, 225)

            click(35, 31)
            click(200, 126)  # Background dimmer
            snapshot('wallpapers-settings')
            click(36, 126)  # Restore brightness before capturing the dock.
            keys(0x1b)
            time.sleep(.7)
            # A second click after closing must reopen the window. Return to
            # Downloaded before taking the final frame so the UI is familiar.
            click_button(hwnd, 'wallpapers')
            time.sleep(.9)
            click(60, 83)
            time.sleep(.35)
            snapshot('wallpapers-reopened')
            click(1223, 31)
            time.sleep(.6)
            dock = grab((0, height - round(245 * dock_scale), width, height))
            dock.save(self.folder / 'dock.png')
            print(f'Play hover, modal, direct apply, search, playlist rotation, dimmer and close passed: {self.folder}', flush=True)
        finally:
            u.SetWindowPos(hwnd, -2, 0, 0, 0, 0, 0x13)
            u.SetCursorPos(previous.x, previous.y)


if __name__ == '__main__':
    run.Playlist = Check
    sys.argv.append('--fixtures')
    run.main()
