"""Open the native instance dialogs and exercise creation and editing in a temporary profile."""
import argparse
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from PIL import ImageChops, ImageStat
from verify_home_widgets import ImageGrab, Input, Mouse, collection_fixture, run, test_video, u


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", action="store_true", help="Verify the live Forge and Adoptium lists, and save their selections.")
    parser.add_argument("--art", action="store_true", help="Verify both artwork pickers and their saved selections.")
    parser.add_argument("--console", action="store_true", help="Verify the floating console and the detailed log view.")
    parser.add_argument("--picker-crt", action="store_true", help="Verify the artwork CRT separately from the online catalog.")
    parser.add_argument("--reference", action="store_true",
                        help="Verify the CRT, Home clock, Better MC slideshow and version drum.")
    parser.add_argument("--pack", choices=["curseforge", "modrinth"], nargs="?", const="modrinth",
                        help="Install Fabulously Optimized from the chosen catalog in the test profile.")
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix="instances-native-", dir=run.ROOT / "scratch"))
    print(root, flush=True)
    env = dict(os.environ, APPDATA=str(root / "roaming"), LOCALAPPDATA=str(root / "local"),
               USERPROFILE=str(root / "profile"), IKA_PRESENT_COPY="1", IKA_DCOMP_VIDEO="1")
    settings = collection_fixture(env)
    if args.console:
        for instance in (root / "roaming/IkaLauncher/instances").iterdir():
            logs = instance / "logs"
            logs.mkdir()
            (logs / "latest.log").write_text(
                "[11:20:03] [Render thread/WARN]: Aviso de la consola nativa\n"
                "[11:20:06] [Server thread/ERROR]: Error de la consola nativa\n", encoding="utf-8")
    u.SetProcessDpiAwarenessContext.argtypes = [c.c_void_p]
    u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
    if args.reference:
        desktop_dpi = u.GetDpiForSystem() / 96
        desktop_w, desktop_h = (u.GetSystemMetrics(i) / desktop_dpi for i in (0, 1))
        frame_w = min(max(780, desktop_w*.94), 1280, desktop_w-12)
        # Place the 710-point clock across the left edge of the window.
        clock_left = max(10, (desktop_w-frame_w)/2 - 340)
        clock_top = max(14, (desktop_h-900)/2 - 20)
        (settings / "home-clock.json").write_text(json.dumps({
            "style": "minecraft", "left": clock_left, "top": clock_top, "clock24h": True,
            "scale": 2.5, "width": 710, "height": 287.5,
            "viewportWidth": desktop_w, "viewportHeight": desktop_h,
        }), encoding="utf-8")
    video = test_video(root)
    worker = run.Worker(run.Media.from_path(video), 0, 0, root, root, run.BIN, None, False, root, 60)
    log = root / "viewer.log"
    previous = w.POINT()
    u.GetCursorPos(c.byref(previous))
    viewer = None
    try:
        with log.open("w", encoding="utf8") as handle:
            viewer = subprocess.Popen([run.ROOT / "target/release/ika-wgpu-wallpaper-bridge.exe",
                                       "--shared-source", worker.info], env=env, stdout=handle, stderr=handle,
                                      creationflags=subprocess.CREATE_NO_WINDOW)
        hwnd, = run.wait_for(lambda: run.windows_on_display(viewer.pid)
                             if "Pantalla de carga cerrada" in log.read_text(encoding="utf8") else None,
                             worker.process, viewer, timeout=35)
        assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x03)
        u.SetForegroundWindow(hwnd)
        time.sleep(.75)
        bounds, origin = w.RECT(), w.POINT()
        assert u.GetClientRect(hwnd, c.byref(bounds)) and u.ClientToScreen(hwnd, c.byref(origin))
        width, height = bounds.right, bounds.bottom
        dpi = u.GetDpiForWindow(hwnd) / 96
        dock_scale = min(dpi, width / 2160, height / 560)

        def tap(x, y):
            assert u.SetCursorPos(origin.x + round(x), origin.y + round(y))
            time.sleep(.06)
            for flag in (2, 4):
                mouse = Input(0, Mouse(0, 0, 0, flag, 0, 0))
                assert u.SendInput(1, c.byref(mouse), c.sizeof(Input)) == 1
                time.sleep(.045)

        def key(code):
            u.keybd_event(code, 0, 0, 0)
            u.keybd_event(code, 0, 2, 0)

        def type_name(name):
            u.keybd_event(0x11, 0, 0, 0)
            key(0x41)
            u.keybd_event(0x11, 0, 2, 0)
            for letter in name:
                if letter.isupper():
                    u.keybd_event(0x10, 0, 0, 0)
                key(0xBE if letter == "." else ord(letter.upper()))
                if letter.isupper():
                    u.keybd_event(0x10, 0, 2, 0)
                time.sleep(.015)

        def save(name, box=None):
            b = (0, 0, width, height) if box is None else box
            image = ImageGrab.grab(tuple(round(n) for n in
                                   (origin.x + b[0], origin.y + b[1], origin.x + b[2], origin.y + b[3])),
                                   all_screens=True).convert("RGB")
            if name:
                image.save(root / f"{name}.png")
            return image

        if args.console:
            def gesture(x, y, dx, dy):
                u.SetCursorPos(origin.x + round(x*dpi), origin.y + round(y*dpi))
                time.sleep(.08)
                for flag in (2, 4):
                    mouse = Input(0, Mouse(0, 0, 0, flag, 0, 0))
                    assert u.SendInput(1, c.byref(mouse), c.sizeof(Input)) == 1
                    if flag == 2:
                        for n in range(1, 9):
                            time.sleep(.032)
                            u.SetCursorPos(origin.x + round((x+dx*n/8)*dpi), origin.y + round((y+dy*n/8)*dpi))
                        time.sleep(.08)
                time.sleep(.15)

            def logical(x, y):
                tap(x*dpi, y*dpi)

            def console_state(predicate):
                def ready():
                    state_file = settings / "home-console.json"
                    state = json.loads(state_file.read_text(encoding="utf-8")) if state_file.is_file() else None
                    return state if state and predicate(state) else None
                return run.wait_for(ready, viewer, timeout=3)

            x, y = width/dpi - 652., 112.
            save("consola-inicio")
            logical(x+570,y+28)
            open_volume = save("consola-volumen")
            assert open_volume.getpixel((round((x+415)*dpi),round((y+70)*dpi))) == (17,14,35)
            logical(x+607,y+28)
            console_state(lambda s: s["minimized"])
            save("consola-minimizada")
            logical(x+24,y+24)
            console_state(lambda s: not s["minimized"])
            gesture(x+160,y+26,-90,25)
            mx, my = console_state(lambda s: s["origin"][0] < x-50)["origin"]
            assert (mx-x) < -50 and (my-y) > 10, (x,y,mx,my)
            gesture(mx+632,my+352,56,25)
            cw, ch = console_state(lambda s: s["size"][0] > 670)["size"]
            assert cw > 670 and ch > 373, (cw,ch)
            logical(mx+cw-124,my+ch-36)
            time.sleep(.65)
            detail = save("consola-detallada")
            assert (212,49,60) in detail.getdata() and (180,100,4) in detail.getdata(), "Los registros de la instancia no llegaron a la consola"
            assert "panicked" not in log.read_text(encoding="utf8")
            print("La consola flotante responde y la vista detallada muestra los registros de la instancia.", flush=True)
            return

        def dock(x):
            tap(width / 2 + (x - 1280) * dock_scale, height - 139 * dock_scale)

        scale = min(dpi, width / 800, height / 725)
        vw, vh = width/scale, height/scale
        mw, mh = min(max(780, vw*.94), 1280, vw-12), min(840, max(540, vh-259))
        sx, sy = (vw-mw)/2, (vh-51-mh)/2
        footer = (sx+mw-110, sy+mh-35)
        def dialog_tap(x,y):
            tap((sx+x)*scale, (sy+y)*scale)
        def form(name, until=None):
            start = time.monotonic()
            while True:
                image = save(None, (sx*scale, sy*scale, (sx+mw)*scale, (sy+mh)*scale))
                sample = image.getpixel((round(mw*scale*.5),50))
                visible = all(abs(v-t) <= 2 for v,t in zip(sample,(4,4,22)))
                ready = visible and (until is None or until(image))
                if ready or time.monotonic() - start >= 2:
                    if name:
                        image.save(root / f"{name}.png")
                    assert ready, (name, sample)
                    return image
                time.sleep(.012)

        def picker_bounds(cover):
            pw, ph = (min(vw*.97,1320), min(vh*.95,920)) if cover else (min(vw*.92,520), min(vh*.8,466))
            px, py = (vw-pw)/2, (vh-ph)/2
            return px, py, pw, ph

        def picker(cover, name):
            px, py, pw, ph = picker_bounds(cover)
            box = (px*scale,py*scale,(px+pw)*scale,(py+ph)*scale)
            start = time.monotonic()
            early = []
            while True:
                image = save(None, box)
                sample = image.getpixel((round(pw*scale*.5),round(49*scale)))
                visible = all(abs(v-t) <= 2 for v,t in zip(sample,(4,4,22)))
                early.append(visible)
                if visible and time.monotonic() - start >= .46:
                    image.save(root / f"{name}.png")
                    break
                if time.monotonic() - start >= 2:
                    image.save(root / f"{name}.png")
                    raise AssertionError((name, sample))
                time.sleep(.025)
            if (args.reference or args.picker_crt) and name != "portada-ampliada":
                assert not all(early), f"{name} se abrió sin transición CRT"
            return pw, ph, lambda x,y: tap((px+x)*scale,(py+y)*scale)

        def close_picker(cover, name):
            px, py, pw, ph = picker_bounds(cover)
            cy = (py+ph/2)*scale
            strip = ((px+pw*.20)*scale, cy-8, (px+pw*.80)*scale, cy+8)
            key(0x1b)
            end = time.monotonic() + .60
            brightest = 0
            frames = []
            while time.monotonic() < end:
                beam = save(None, strip)
                frames.append(beam)
                mask = beam.point(lambda n: 255 if n > 140 else 0)
                mask = mask.convert("L").tobytes()
                bright = max(mask[y:y+beam.width].count(255) for y in range(0, len(mask), beam.width))
                brightest = max(bright, brightest)
            if frames:
                from PIL import Image
                timeline = Image.new("RGB", (beam.width, (beam.height+2)*len(frames)))
                for i, shot in enumerate(frames):
                    timeline.paste(shot, (0, i*(beam.height+2)))
                timeline.save(root / f"{name}-haz.png")
            assert brightest > beam.width*.34, (name, brightest, beam.width)
            form(f"{name}-cerrado")

        col = (mw - 68) / 2
        cover_h = min(144, max((mh-158)*.37-73,62))
        loader_y = 78 + cover_h + 7 + 68 + 79
        pack_h = mh - 80 - loader_y
        grid_y = loader_y + (78 if pack_h < 180 else 93)
        cell_w = (col - 2 - 28) / 5
        cell_h = (mh - 81 - grid_y - 14) / 3

        def artwork_tiles(image):
            tiles = []
            for index in range(15):
                x = 25 + index % 5 * (cell_w + 7)
                y = grid_y + index // 5 * (cell_h + 7)
                if cell_h < 49:
                    tw = min(24, cell_h-10)
                    box = (x + 6, y + 7, x + tw + 2, y + cell_h - 7)
                else:
                    box = (x + 8, y + 8, x + cell_w - 8, y + cell_h - 27)
                tile = image.crop(tuple(round(coord*scale) for coord in box))
                assert len(set(tile.getdata())) > 15, ("No se encontró la portada", index, box)
                tiles.append(tile)
            return tiles

        # Create from the original toolbar pill above the carousel.
        area_bot = height - 200 * dock_scale
        home_scale = min(dpi, width / 1644, (area_bot - 105*dpi) / 380)
        toolbar_scale = min(dpi, width / 788)
        if args.reference:
            clock_x = max(10, clock_left * (width/dpi-710) / max(1, desktop_w-710))
            clock_y = 52 + max(14, clock_top * (height/dpi-52-287.5) / max(1, desktop_h-52-287.5))
            clock_box = tuple(round(n * dpi) for n in (
                max(0, clock_x-10), clock_y+112, min(width/dpi, clock_x+720), clock_y+280,
            ))
            clock = save("clock-home", clock_box)
            def assert_clock(name, shown):
                overlay = save(name, clock_box)
                a, b = list(clock.getdata()), list(overlay.getdata())
                stripe = {x for x in range(clock.width)
                          if sum(min(p) > 224 for p in a[x::clock.width]) > clock.height/2}
                ink = [i for i,p in enumerate(a) if i % clock.width not in stripe and min(p) > 224]
                assert len(ink) > 120, "No se encontraron las cifras blancas del reloj"
                coverage = sum(min(b[i]) > 210 for i in ink) / len(ink)
                assert coverage > .78 if shown else coverage < .24, (name, coverage)

            def assert_clock_behind(name, left, dimmed):
                overlay = save(name, clock_box)
                a, b = list(clock.getdata()), list(overlay.getdata())
                outside, inside = [], []
                for i, pixel in enumerate(a):
                    if min(pixel) < 246:
                        continue
                    x = clock_box[0] + i % clock.width
                    if x < left - 8*dpi:
                        outside.append(i)
                    elif x > left + 16*dpi:
                        inside.append(i)
                assert min(len(outside), len(inside)) > 120, (name, len(outside), len(inside))
                def is_clock_ink(pixel):
                    # The black veil leaves white clock ink at (109,109,109).
                    # Bright labels and the modpack art inside are not that ink.
                    return (max(abs(v-109) for v in pixel) < 9 if dimmed
                            else min(pixel) > 235)
                visible = sum(is_clock_ink(b[i]) for i in outside) / len(outside)
                covered = sum(is_clock_ink(b[i]) for i in inside) / len(inside)
                assert visible > .72 and covered < .24, (name, visible, covered)

            wall_scale = min(dpi, width/1326, (height-190*dock_scale)/540)
            tap(width-174*dock_scale, height-80*dock_scale)
            time.sleep(.95)
            assert_clock_behind("clock-wallpapers", (width-1260*wall_scale)/2, False)
            key(0x1b)
            time.sleep(.65)
            assert_clock("clock-after-wallpapers", True)

        tap(width/2 - 278*toolbar_scale, area_bot - 355*home_scale)
        if args.reference:
            early = save("create-opening", (sx*scale, sy*scale, (sx+mw)*scale, (sy+mh)*scale))
            assert early.getpixel((early.width//2, round(20*scale))) != (4,4,22), "Crear se abrió sin CRT"
        time.sleep(.81)
        first_frame = form("create-first")
        if args.picker_crt:
            for mode in ("crear", "editar"):
                dialog_tap(24+col/2,78+cover_h/2)
                picker(True, f"{mode}-portada")
                close_picker(True, f"{mode}-portada")
                dialog_tap(57,78+cover_h+36)
                picker(False, f"{mode}-icono")
                close_picker(False, f"{mode}-icono")
                key(0x1b)
                time.sleep(.7)
                if mode == "crear":
                    dock(1533)
                    time.sleep(.81)
                    form("edit")
            assert "panicked" not in log.read_text(encoding="utf8")
            print("Portada e icono se abren y cierran con CRT en Crear y Editar.", flush=True)
            return
        first = artwork_tiles(first_frame)
        time.sleep(2)
        settled = artwork_tiles(form("create"))
        # The opening transition and compositor can round colors differently.
        for a, b in zip(first, settled):
            diff = ImageChops.difference(a, b)
            assert max(ImageStat.Stat(diff).mean) < 4, "La red cambió una portada visible"
        if args.reference:
            assert_clock_behind("clock-create", sx*scale, True)
            save("clock-create-layering")
            border_start = form(None)
            time.sleep(.72)
            moved = form("create-border-moving")
            # The same stripe should be farther right in the later frame.
            # Track the color directly at the edge, away from the corners.
            left, right, y = round(100*scale), round((mw-255)*scale), round(scale)
            stripe = border_start.crop((left, y, right, y+1))
            def border_error(dx):
                patch = moved.crop((left+dx, y, right+dx, y+1))
                return sum(ImageStat.Stat(ImageChops.difference(stripe, patch)).mean)
            shifted_error = min(border_error(d) for d in range(round(56*scale), round(145*scale)))
            assert shifted_error < border_error(0) * .5, "El borde no se deslizó hacia la derecha"
            icon_y = 78 + cover_h + 11
            rim = tuple(round(n*scale) for n in (42, icon_y, 72, icon_y+2))
            art = tuple(round(n*scale) for n in (42, icon_y+9, 72, icon_y+41))
            def icon_difference(box):
                return max(ImageStat.Stat(ImageChops.difference(
                    border_start.crop(box), moved.crop(box))).mean)
            assert icon_difference(rim) > 3, "El gradiente del icono no se movió"
            assert icon_difference(art) < 1, "La imagen del icono cambió con su borde"
            catalog = json.loads((run.ROOT / "src/home/modpack-catalog/curseforge.json").read_text(encoding="utf-8"))
            index = next(i for i,p in enumerate(catalog["projects"][:15]) if p["title"] == "Better MC [FORGE] BMC4")
            dialog_tap(25+(index%5)*(cell_w+7)+cell_w/2, grid_y+(index//5)*(cell_h+7)+cell_h/2)
            right_h = mh-158
            hero_h = min(190, max(80, right_h*(.30 if right_h < 460 else .34)))
            x, y = 44+col, 78+hero_h+6
            def official_gallery():
                image = save(None, ((sx+x+9)*scale, (sy+y)*scale, (sx+x+244)*scale, (sy+y+26)*scale))
                return image if all(len(set(image.crop((int(i*60*scale),0,int(i*60*scale+40*scale),image.height)).getdata())) > 100 for i in range(4)) else None
            run.wait_for(official_gallery, viewer, timeout=18)
            dialog_tap(x+26, y+13)  # Restart the slideshow from the first thumbnail.
            time.sleep(.45)
            bmc = form("better-mc")
            track_y = round((78+hero_h-1)*scale)
            def progress_width(image):
                start, end = round((x+2)*scale), round((x+col-9)*scale)
                for i in range(start, end):
                    r,g,b = image.getpixel((i, track_y))
                    if not (114 <= r <= 208 and 48 <= g <= 151 and b >= 223 and b-r >= 26):
                        return i-start
                return end-start
            time.sleep(.5)
            progress = form("better-mc-progress")
            assert progress_width(progress)-progress_width(bmc) > col*scale*.10, "La línea no avanzó"
            hero = tuple(round(n*scale) for n in (x+28, 93, x+col-24, 65+hero_h))
            def hero_changed(frame):
                return max(ImageStat.Stat(ImageChops.difference(bmc.crop(hero), frame.crop(hero))).mean) > 12
            form("better-mc-autoplay", hero_changed)
            time.sleep(.55)
            dialog_tap(x+9+60*2+26, y+13)
            time.sleep(.65)
            form("better-mc-third")
            dialog_tap(24+col*.78, 78+cover_h+7+68+47)  # Custom
            drum_w = (col-12)/2
            drum_y = loader_y + (86 + mh - 80 - loader_y - 6)/2
            dialog_tap(100, loader_y+68)
            type_name("1.20.1")
            time.sleep(.2)
            u.keybd_event(0x11, 0, 0, 0)
            key(0x41)
            u.keybd_event(0x11, 0, 2, 0)
            key(0x08)
            time.sleep(.23)
            lens = tuple(round(n*scale) for n in (24+drum_w*.36, drum_y-15, 24+drum_w*.64, drum_y+15))
            before_scroll = form("minecraft-version").crop(lens)
            assert u.SetCursorPos(origin.x+round((sx+41)*scale), origin.y+round((sy+drum_y)*scale))
            time.sleep(.1)
            wheel = Input(0, Mouse(0, 0, (-120)&0xffffffff, 0x800, 0, 0))
            assert u.SendInput(1, c.byref(wheel), c.sizeof(Input)) == 1
            time.sleep(.4)
            after_scroll = form("minecraft-version-scrolled").crop(lens)
            assert max(ImageStat.Stat(ImageChops.difference(before_scroll, after_scroll)).mean) > 2
            key(0x1b)
            time.sleep(.08)
            save("clock-closing", clock_box)
            time.sleep(.52)
            assert_clock("clock-back-home", True)
            tap(width/2-205*dpi, 67*dpi)  # Comparador
            time.sleep(.30)
            assert_clock("clock-other-section", False)
            tap(width/2-322*dpi, 67*dpi)  # Inicio
            time.sleep(.30)
            assert_clock("clock-home-restored", True)
            tap(width/2 - 278*toolbar_scale, area_bot - 355*home_scale)
            time.sleep(.81)
            form("create-reopened")
        if not args.pack:
            modrinth_w = min(91, max(77, col*.15))
            dialog_tap(24+col-modrinth_w/2, loader_y+32)
            # Compare the instance icon too: it must update in the very same
            # frame as the catalog, rather than showing the previous pack name/art.
            icon_box = tuple(round(n*scale) for n in (41, 78+cover_h+21, 73, 78+cover_h+51))
            first_icon = first_frame.crop(icon_box)
            modrinth_frame = form("create-modrinth", lambda frame:
                max(ImageStat.Stat(ImageChops.difference(first_icon, frame.crop(icon_box))).mean) > 7)
            icon = modrinth_frame.crop(icon_box)
            assert len(set(icon.getdata())) > 15
            modrinth = artwork_tiles(modrinth_frame)
            assert all(ImageChops.difference(a,b).getbbox() is not None for a,b in zip(first, modrinth))
        if args.pack:
            search_y = loader_y + 31
            modrinth_w = min(91, max(77, col*.15))
            if args.pack == "modrinth":
                dialog_tap(24+col-modrinth_w/2, search_y)
            dialog_tap(24+col*.24, search_y)
            type_name("Fabulously Optimized")
            key(0x0D)
            time.sleep(4)
            form(args.pack)
            tap(footer[0]*scale,footer[1]*scale)
            pack_cfg = Path(env["APPDATA"]) / "IkaLauncher/instances/Fabulously Optimized/instance.cfg"
            try:
                run.wait_for(lambda: pack_cfg.is_file(), viewer, timeout=95)
            except Exception:
                save("install-failure")
                raise
            assert f"ManagedPackType={args.pack}" in pack_cfg.read_text(encoding="utf8")
            assert list((pack_cfg.parent / ".minecraft/mods").glob("*.jar"))
            print(f"Fabulously Optimized instalado desde {args.pack}.", flush=True)
            return
        name_y = 78 + cover_h + 7 + 42
        dialog_tap(24+col*.78, 78+cover_h+7+68+47)  # Custom
        dialog_tap(190, name_y)
        type_name("NativeProof")
        time.sleep(.3)
        form("create-custom")
        if args.art:
            dialog_tap(24+col/2,78+cover_h/2)
            pw, ph, pick_tap = picker(True, "elegir-portada")
            pick_tap(pw*.63,210)  # Fondo 2, to the right of the expanded Fondo 1.
            time.sleep(.5)
            picker(True, "portada-ampliada")
            pick_tap(pw-100,ph-36)
            time.sleep(.46)
            dialog_tap(57,78+cover_h+36)
            pw, _, pick_tap = picker(False, "elegir-icono")
            pick_tap(44,173)
            time.sleep(.46)
            form("arte-seleccionado")
        loader_x = 24 + (col-12)/2 + 12
        if args.catalog:
            loader_w = (col-12)/2
            slot = (loader_w-42)/3
            dialog_tap(loader_x+15+2*(slot+6)+slot*.5, loader_y+62)  # Forge
            compact_java = mh - 158 < 460
            dialog_tap(44+col+54, 78+(148 if compact_java else 189))  # Adoptium
            time.sleep(4)
            java_height = min(375, max(172, 220 + (mh-660)*.575))
            java_box = tuple(round(n*scale) for n in (44+col, 78, 44+2*col, 78+java_height))
            create_java = form("create-catalog").crop(java_box)
        else:
            dialog_tap(loader_x+39, loader_y+55)  # Vanilla
        tap(footer[0]*scale,footer[1]*scale)

        instance = Path(env["APPDATA"]) / "IkaLauncher/instances/NativeProof"
        try:
            run.wait_for(lambda: (instance / "mmc-pack.json").is_file(), viewer, timeout=10)
        except TimeoutError:
            save("create-save-failure")
            raise
        pack = json.loads((instance / "mmc-pack.json").read_text(encoding="utf8"))
        expected = ["net.minecraft", "net.minecraftforge"] if args.catalog else ["net.minecraft"]
        assert [p["uid"] for p in pack["components"]] == expected, pack
        if args.catalog:
            cfg = (instance / "instance.cfg").read_text(encoding="utf8")
            assert "IkaJavaVendor=adoptium" in cfg and "IkaJavaRuntime=eclipse_temurin_" in cfg, cfg
        if args.art:
            ini = (instance / "instance.cfg").read_text(encoding="utf8")
            assert "IkaCoverPreset=shader_complementary_reimagined_2" in ini, ini
            assert any(line.startswith("IkaIconPreset=") and line != "IkaIconPreset=" for line in ini.splitlines()), ini
            first_icon = (instance / "ika-icon.png").read_bytes()
            assert first_icon and (instance / "ika-cover.png").is_file()
        time.sleep(.6)

        if args.reference:
            clock = save("clock-before-edit", clock_box)
        dock(1533)
        if args.reference:
            early = save("edit-opening", (sx*scale, sy*scale, (sx+mw)*scale, (sy+mh)*scale))
            assert early.getpixel((early.width//2, round(20*scale))) != (4,4,22), "Editar se abrió sin CRT"
        time.sleep(.81)
        form("edit")
        if args.reference:
            assert_clock_behind("clock-edit", sx*scale, True)
        if args.art:
            dialog_tap(24+col/2,78+cover_h/2)
            picker(True, "editar-portada")
            key(0x1B)  # Close only the artwork picker.
            time.sleep(.46)
            form("editar-despues-de-escape")
            dialog_tap(57,78+cover_h+36)
            pw, _, pick_tap = picker(False, "editar-icono")
            pick_tap(pw-73,89)  # Random icon.
            time.sleep(.46)
            form("editar-icono-seleccionado")
        dialog_tap(190,name_y)
        type_name("EditedProof")
        edit_changed = form("edit-changed")
        if args.catalog and args.art:
            edit_java = edit_changed.crop(java_box)
            assert max(ImageStat.Stat(ImageChops.difference(create_java, edit_java)).mean) < 1, "El panel de Java se dibujó distinto en Editar"
            edit_java.save(root / "java-compartido.png")
        tap(footer[0]*scale,footer[1]*scale)
        cfg = instance / "instance.cfg"
        run.wait_for(lambda: "name=EditedProof" in cfg.read_text(encoding="utf8"), viewer, timeout=10)
        if args.art:
            assert (instance / "ika-icon.png").read_bytes() != first_icon
            assert "IkaCoverPreset=shader_complementary_reimagined_2" in cfg.read_text(encoding="utf8")
            print("Portada e icono se conservan al abrir Editar y guardan al elegir otros.", flush=True)
        time.sleep(.65)
        if args.catalog:
            assert "IkaJavaVendor=adoptium" in cfg.read_text(encoding="utf8")
            assert json.loads((instance / "mmc-pack.json").read_text(encoding="utf8")) == pack
            print("Forge y Adoptium se guardaron y se conservaron al editar la instancia.", flush=True)
            return

        dock(919)
        time.sleep(1.05)
        es = min(dpi,width/610,height/518)
        x = (width-580*es)/2
        y = max(8*es, (height-614*es)/2)
        image = save("export", (x,y,x+580*es,y+466*es))
        assert image.getpixel((round(300*es),round(49*es))) == (4,4,22)
        zip_slot = image.crop(tuple(round(n*es) for n in (28,406,142,446)))
        assert all(max(abs(v-t) for v,t in zip(p,(4,4,22))) < 3 for p in zip_slot.getdata()), "Guardar ZIP sigue visible"
        key(0x1b)
        time.sleep(.8)
        assert "panicked" not in log.read_text(encoding="utf8")
        print("Crear y editar guardan sus cambios; Exportar abre desde el dock.", flush=True)
    finally:
        run.stop(viewer)
        worker.close()
        u.SetCursorPos(previous.x, previous.y)


if __name__ == "__main__":
    main()
