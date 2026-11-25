"""Exercise Home widgets and sample both sides of an MP4 during UI redraws."""

import argparse
import base64
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from PIL import Image, ImageChops, ImageGrab

import run

sys.path.insert(0, str(run.ROOT))
from verify import USER32 as u, Input, Mouse


ICONS = [(225, 52, 101), (36, 211, 102), (54, 103, 225), (229, 190, 45), (193, 54, 214)]


def collection_fixture(env):
    settings = Path(env['LOCALAPPDATA']) / 'IkaNative'
    settings.mkdir(parents=True)
    steve = base64.b64encode((run.ROOT / 'src/profile/steve.png').read_bytes()).decode('ascii')
    (settings / 'profiles.json').write_text(json.dumps({
        'added': [{'username': 'PanelTest', 'displayName': 'Panel Test',
                   'customSkinData': 'data:image/png;base64,' + steve}],
    }), encoding='utf-8')
    fixtures = [
        ('01 Aurora', '1.20.1', 'Fabric'), ('02 Brisa', '1.21.1', 'Forge'),
        ('03 Cielo', '1.21.1', 'Fabric'), ('04 Delta Pack', '1.20.1', 'Forge'),
        ('05 Eclipse', '1.21.1', 'Fabric'), ('06 Faro', '1.20.1', 'Vanilla'),
    ]
    for i, (name, version, loader) in enumerate(fixtures):
        folder = Path(env['APPDATA']) / 'IkaLauncher/instances' / name
        folder.mkdir(parents=True)
        (folder / 'instance.cfg').write_text(
            f'InstanceType=OneSix\nname={name}\nIntendedVersion={version}\nIntendedLoader={loader}\n'
            'JavaVendor=Eclipse Adoptium\n', encoding='utf-8',
        )
        if i < len(ICONS):
            Image.new('RGB', (64, 64), ICONS[i]).save(folder / 'icon.png')
    return settings


def test_video(out):
    path = out / 'split-colors.mp4'
    subprocess.run([
        'ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i',
        'color=c=0x3070c0:s=1280x720:r=60:d=2',
        '-vf', 'drawbox=x=640:w=640:h=720:c=0xc08038:t=fill,drawbox=x=615:y=0:w=25:h=720:c=white:t=fill',
        '-an', '-c:v', 'libx264', '-pix_fmt', 'yuv420p', str(path),
    ], check=True, timeout=30)
    return path


def check(out, exe, video, native, widgets, legacy=False):
    out.mkdir()
    env = dict(os.environ, APPDATA=str(out / 'roaming'), LOCALAPPDATA=str(out / 'local'),
               USERPROFILE=str(out / 'profile'),
               IKA_PRESENT_COPY='1', IKA_DCOMP_VIDEO=str(int(native)))
    settings = collection_fixture(env) if widgets else None
    state_path = Path(env['LOCALAPPDATA']) / 'IkaNative/home-clock.json'
    log_path = out / 'viewer.log'
    owner = run.Worker(run.Media.from_path(video), 0, 0, out, out, run.BIN, None, False, out, 60)
    viewer = None
    previous = w.POINT()
    u.GetCursorPos(c.byref(previous))
    try:
        if legacy:
            owner.info.write_text(owner.info.read_text().removesuffix(' video\n'), encoding='ascii')
        with log_path.open('w', encoding='utf-8') as log:
            viewer = subprocess.Popen([exe, '--shared-source', owner.info], env=env, stdout=log, stderr=log,
                                      creationflags=subprocess.CREATE_NO_WINDOW)

        def ready():
            if 'Pantalla de carga cerrada' in log_path.read_text(encoding='utf-8'):
                return run.windows_on_display(viewer.pid)

        hwnd, = run.wait_for(ready, owner.process, viewer, timeout=30)
        assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
        bounds, origin = w.RECT(), w.POINT()
        assert u.GetClientRect(hwnd, c.byref(bounds)) and u.ClientToScreen(hwnd, c.byref(origin))
        width, height = bounds.right, bounds.bottom
        dpi = u.GetDpiForWindow(hwnd) / 96

        def move(x, y):
            assert u.SetCursorPos(origin.x + round(x), origin.y + round(y))

        def click(flags, data=0):
            mouse = Input(0, Mouse(0, 0, data, flags, 0, 0))
            assert u.SendInput(1, c.byref(mouse), c.sizeof(Input)) == 1

        def grab(x, y, right, bottom):
            return ImageGrab.grab(
                (origin.x + round(x), origin.y + round(y), origin.x + round(right), origin.y + round(bottom)),
                all_screens=True,
            ).convert('RGB')

        def screenshot(name):
            grab(0, 0, width, height).save(out / (name + '.png'))

        def tap(x, y, right=False):
            move(x * dpi, y * dpi)
            time.sleep(.08)
            click(8 if right else 2)
            time.sleep(.07)
            click(16 if right else 4)
            time.sleep(.07)

        def state(predicate):
            def read():
                if not state_path.exists():
                    return None
                data = json.loads(state_path.read_text(encoding='utf-8'))
                return data if predicate(data) else None
            return run.wait_for(read, viewer, timeout=5)

        split = video.name == 'split-colors.mp4'
        move(width * .5, height * .3)
        hits = 0
        samples = 90
        for i in range(samples):
            # Change hover while the decoder runs on its own fence cadence.
            if i % 6 == 0:
                move(width * (.46 + (i // 6 % 2) * .07), height * .32)
            if split:
                y = height * .30
                stripe = grab(width * .35, y, width * .88, y + 1)
                left = stripe.getpixel((stripe.width // 6, 0))
                right = stripe.getpixel((stripe.width * 4 // 5, 0))
                hits += int(not (left[2] > left[0] + 45 and right[0] > right[2] + 45))
            time.sleep(.009)
        print(f'{out.name}: {hits}/{samples} incorrect stripe samples' if split else out.name, flush=True)
        if split and not legacy:
            assert hits == 0
        if not widgets:
            return

        def panel(side, open):
            x = (110 * dpi if side == 'left' else width - 110 * dpi)
            y = min(height * .40, 440 * dpi)
            pixel = tuple(grab(x, y, x + 1, y + 1).getpixel((0, 0)))
            return (max(pixel) < 85) == open

        scale = min(dpi, width / 2160, height / 560)
        rail = round(height - (340 - 134 - 34 * 134 / 148) * scale - 1)
        dock_area = (24 * scale, height - 123 * scale, 400 * scale, height - 43 * scale)
        dock_closed = grab(*dock_area)
        for side in ['left', 'right']:
            move(5 if side == 'left' else width - 5, 240 * dpi)
            run.wait_for(lambda: panel(side, True), viewer, timeout=5)
            time.sleep(.45)
            screenshot(side)
            sample_x = 6 * dpi if side == 'left' else width - 7 * dpi
            sample_y = 74 * dpi
            assert grab(sample_x, sample_y, sample_x + 1, sample_y + 1).getpixel((0, 0)) == (4, 4, 22)
            # The last row inside the panel keeps its fill; the rail below belongs to the dock.
            x = 100 * dpi if side == 'left' else width - 100 * dpi
            strip = grab(x, rail - 5, x + 1, rail)
            top, bottom = strip.getpixel((0, 0)), strip.getpixel((0, strip.height - 1))
            assert max(abs(a - b) for a, b in zip(top, bottom)) < 30, (side, top, bottom)
            if side == 'left':
                dock_open = grab(*dock_area)
                diff = ImageChops.difference(dock_closed, dock_open)
                matched = min(sum(channel.histogram()[:8]) for channel in diff.split())
                total = dock_open.width * dock_open.height
                assert matched / total > .98, f'Dock cubierto: {matched}/{total} píxeles conservados'

                # Five square tiles in the first row; the sixth uses a Minecraft block.
                collection_top = 52 + 12 + 165 + 12
                span = min(417, width / dpi * .85)
                cell = (span - 24 - 28 - 4 * 8) / 5
                first_x = 26 + cell / 2
                row_y = collection_top + 85 + cell / 2

                def tile(index, y=row_y):
                    x = (first_x + (cell + 8) * (index % 5)) * dpi
                    y = (y + (cell + 8) * (index // 5)) * dpi
                    return grab(x, y, x + 1, y + 1).getpixel((0, 0))

                def same_color(actual, expected):
                    return max(abs(a - b) for a, b in zip(actual, expected)) <= 5

                assert all(same_color(tile(i), color) for i, color in enumerate(ICONS)), [
                    tile(i) for i in range(len(ICONS))
                ]
                assert max(tile(5)) > 42, f'Falta el bloque de Minecraft: {tile(5)}'

                tap(first_x + cell + 8, row_y)
                path = settings / 'minecraft-home.json'
                run.wait_for(lambda: path.exists() and
                             json.loads(path.read_text(encoding='utf-8'))['selected'].endswith('02 Brisa'),
                             viewer, timeout=5)
                time.sleep(.2)
                screenshot('collection-selected')

                # A right click shows the actual tile menu and persists the favorite.
                tap(first_x + cell + 8, row_y, right=True)
                tap(first_x + cell + 24, row_y + 13)
                favorites = settings / 'home-favorites.json'
                run.wait_for(lambda: favorites.exists() and
                             json.loads(favorites.read_text(encoding='utf-8'))[0].endswith('02 brisa'),
                             viewer, timeout=5)
                nav_x = span / 2 - 145
                nav_y = collection_top + 59
                tap(nav_x + 262, nav_y)
                run.wait_for(lambda: same_color(tile(0), ICONS[1]), viewer, timeout=5)
                tap(nav_x + 198, nav_y)
                run.wait_for(lambda: same_color(tile(0), ICONS[3]), viewer, timeout=5)
                tap(nav_x + 78, nav_y)
                tap(nav_x + 12, nav_y)
                tap(62, collection_top + 90)
                tap(62, collection_top + 164)  # Version 1.21.1, after Todas and 1.20.1.
                run.wait_for(lambda: same_color(tile(0, row_y + 34), ICONS[1]), viewer, timeout=5)
                screenshot('collection-filters')
                tap(392, collection_top + 90)  # Reset version filter.
                tap(nav_x + 12, nav_y)
                tap(nav_x + 42, nav_y)
                for code in b'ECLIPSE':
                    u.keybd_event(code, 0, 0, 0)
                    u.keybd_event(code, 0, 2, 0)
                run.wait_for(lambda: same_color(tile(0, row_y + 34), ICONS[4]), viewer, timeout=5)
                screenshot('collection-search')
                print('Dock conservado; cuadrícula, selección, favoritos, filtros y búsqueda verificados', flush=True)
            tap(width * .55 / dpi, height * .27 / dpi)
            run.wait_for(lambda: panel(side, False), viewer, timeout=7)

        w_log, h_log = width / dpi, height / dpi
        x = max(10, 229 * (w_log - 284) / (2560 - 284))
        y = max(66, 52 + 134 * (h_log - 52 - 115) / (1382 - 52 - 115))
        screenshot('clock-before')
        move((x + 277) * dpi, (y + 108) * dpi)
        time.sleep(.1)
        click(2)
        time.sleep(.1)
        move((x + 392) * dpi, (y + 155) * dpi)
        time.sleep(.1)
        click(4)
        larger = state(lambda data: data['scale'] > 1.25)
        screenshot('clock-larger')

        move((larger['left'] + 70) * dpi, (52 + larger['top'] + 55) * dpi)
        time.sleep(.1)
        u.keybd_event(0x11, 0, 0, 0)
        try:
            click(0x0800, 120)
            larger = state(lambda data: data['scale'] > larger['scale'])
        finally:
            u.keybd_event(0x11, 0, 2, 0)

        menu_x, menu_y = larger['left'] + 70, 52 + larger['top'] + 55
        tap(menu_x, menu_y, right=True)
        time.sleep(.25)
        screenshot('styles')

        # Use the menu's format control, then dock the enlarged clock.
        tap(menu_x + 60, menu_y + 157)
        state(lambda data: data['clock24h'])

        tap(menu_x, menu_y, right=True)
        tap(menu_x + 174, menu_y + 22)
        minecraft = state(lambda data: data['style'] == 'minecraft')
        move((minecraft['left'] + 70) * dpi, (52 + minecraft['top'] + 55) * dpi)
        u.keybd_event(0x11, 0, 0, 0)
        try:
            for _ in range(5):
                if minecraft['scale'] > 2:
                    break
                click(0x0800, 360)
                minecraft = state(lambda data: data['scale'] > minecraft['scale'])
        finally:
            u.keybd_event(0x11, 0, 2, 0)
        larger = state(lambda data: data['style'] == 'minecraft' and data['scale'] > 2)
        move(width * .53, height * .22)
        time.sleep(.25)
        x, y = larger['left'], 52 + larger['top']
        clock = grab(x * dpi, y * dpi, (x + larger['width']) * dpi, (y + larger['height']) * dpi)
        clock.save(out / 'clock-minecraft.png')
        # The plain clock has a tall white digit row and a separate, smaller date.
        pixel_scale = larger['scale'] * dpi
        for top, bottom, minimum_height, label in [
            (10, 87, 63, 'hora'), (88, 115, 11, 'fecha'),
        ]:
            band = clock.crop((0, round(top * pixel_scale), clock.width,
                               min(clock.height, round(bottom * pixel_scale))))
            channels = band.split()
            white = ImageChops.darker(ImageChops.darker(*channels[:2]), channels[2])
            bounds = white.point(lambda value: 255 if value >= 218 else 0).getbbox()
            assert bounds and bounds[3] - bounds[1] >= minimum_height * pixel_scale, (label, bounds)

        move((larger['left'] + 70) * dpi, (52 + larger['top'] + 55) * dpi)
        time.sleep(.1)
        click(2)
        time.sleep(.1)
        move(width - 6, height * .25)
        time.sleep(.45)
        click(4)
        docked = state(lambda data: data['dockedSide'] == 'right')
        assert docked['scale'] == larger['scale']
        screenshot('clock-docked')
        print(f'Paneles y reloj Minecraft verificados: escala {larger["scale"]:.2f}, anclaje derecho', flush=True)
    finally:
        if viewer:
            run.stop(viewer, close_window=True)
        owner.close()
        u.SetCursorPos(previous.x, previous.y)


def check_launcher(out, video):
    env = dict(os.environ, APPDATA=str(out / 'launcher-roaming'), LOCALAPPDATA=str(out / 'launcher-local'))
    log_path = out / 'launcher.log'
    with log_path.open('w', encoding='utf-8') as log:
        launcher = subprocess.Popen([
            sys.executable, '-X', 'utf8', '-B', run.HERE / 'run.py', '--chisa', video, '--no-save',
        ], stdout=log, stderr=log, env=env, creationflags=subprocess.CREATE_NO_WINDOW)
    window = None
    try:
        def folder():
            for line in log_path.read_text(encoding='utf-8').splitlines():
                if line.startswith('Resultados: '):
                    return Path(line.removeprefix('Resultados: '))

        session = run.wait_for(folder, launcher, timeout=20)
        launcher_log = session / 'viewer.log'
        window = run.wait_for(
            lambda: int((session / 'viewer-hwnd.txt').read_text()) if (session / 'viewer-hwnd.txt').is_file() else None,
            launcher, timeout=20,
        )
        def playing():
            lines = launcher_log.read_text(encoding='utf-8').splitlines()
            return 'Pantalla de carga cerrada' in lines and 'Shared frames rendered: 60' in lines

        run.wait_for(playing, launcher, timeout=25)
        assert (session / 'startup-surface.txt').read_text().split()[-1] == 'video'
        print(f'bridge/run.py: portada cerrada y 60 fotogramas de {video.name}; {session}', flush=True)
        assert u.PostMessageW(window, 0x10, 0, 0)
        assert launcher.wait(timeout=15) == 0
    finally:
        if launcher.poll() is None and window:
            u.PostMessageW(window, 0x10, 0, 0)
        run.stop(launcher)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--viewer', type=Path, default=run.VIEWER)
    p.add_argument('--baseline', type=Path)
    p.add_argument('--mp4', type=Path)
    args = p.parse_args()
    out = Path(tempfile.mkdtemp(prefix='home-widgets-', dir=run.ROOT / 'scratch'))
    print(out, flush=True)
    video = test_video(out)
    u.SetProcessDpiAwarenessContext.argtypes = [c.c_void_p]
    u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
    if args.baseline:
        check(out / 'baseline', args.baseline.resolve(), video, True, False, legacy=True)
    check(out / 'directcomposition', args.viewer.resolve(), video, True, True)
    check(out / 'wgpu', args.viewer.resolve(), video, False, False)
    if args.mp4:
        check_launcher(out, args.mp4.resolve())


if __name__ == '__main__':
    main()
