"""Exercise the demo's native mouse input and compare its actual GPU output."""

import argparse
import ctypes
import ctypes.wintypes as win
import json
import subprocess
import time
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageStat


HERE = Path(__file__).resolve().parent
USER32 = ctypes.WinDLL("user32", use_last_error=True)
WNDENUMPROC = ctypes.WINFUNCTYPE(win.BOOL, win.HWND, win.LPARAM)
USER32.GetWindowThreadProcessId.argtypes = [win.HWND, ctypes.POINTER(win.DWORD)]
USER32.EnumWindows.argtypes = [WNDENUMPROC, win.LPARAM]
USER32.IsWindowVisible.argtypes = [win.HWND]
USER32.GetClientRect.argtypes = [win.HWND, ctypes.POINTER(win.RECT)]
USER32.GetDpiForWindow.argtypes = [win.HWND]
USER32.IsZoomed.argtypes = [win.HWND]
USER32.GetClassNameW.argtypes = [win.HWND, win.LPWSTR, ctypes.c_int]
USER32.PostMessageW.argtypes = [win.HWND, win.UINT, win.WPARAM, win.LPARAM]
USER32.ClientToScreen.argtypes = [win.HWND, ctypes.POINTER(win.POINT)]
USER32.GetCursorPos.argtypes = [ctypes.POINTER(win.POINT)]
USER32.SetCursorPos.argtypes = [ctypes.c_int, ctypes.c_int]
USER32.SetWindowPos.argtypes = [win.HWND, win.HWND, ctypes.c_int, ctypes.c_int,
                                ctypes.c_int, ctypes.c_int, win.UINT]
USER32.WindowFromPoint.argtypes = [win.POINT]
USER32.WindowFromPoint.restype = win.HWND
USER32.GetSystemMetrics.argtypes = [ctypes.c_int]


class Mouse(ctypes.Structure):
    _fields_ = [('x', win.LONG), ('y', win.LONG), ('data', win.DWORD),
                ('flags', win.DWORD), ('time', win.DWORD), ('extra', ctypes.c_size_t)]


class Input(ctypes.Structure):
    _fields_ = [('type', win.DWORD), ('mouse', Mouse)]


USER32.SendInput.argtypes = [win.UINT, ctypes.POINTER(Input), ctypes.c_int]
USER32.SendInput.restype = win.UINT


def wait_for(check, child, seconds=12):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError(f"Demo closed with exit code {child.returncode}; see window.log")
        result = check()
        if result:
            return result
        time.sleep(0.035)
    raise TimeoutError("Demo did not produce the expected result; see window.log")


def windows_of(pid):
    found = []

    @WNDENUMPROC
    def enumerate_window(hwnd, _):
        owner = win.DWORD()
        USER32.GetWindowThreadProcessId(hwnd, ctypes.byref(owner))
        if owner.value == pid and USER32.IsWindowVisible(hwnd):
            name = ctypes.create_unicode_buffer(128)
            USER32.GetClassNameW(hwnd, name, len(name))
            # Winit's Windows event dispatcher also sets WS_VISIBLE. It is
            # infrastructure for delivering messages, with no wgpu surface.
            if name.value != "Winit Thread Event Target":
                found.append(hwnd)
        return True

    USER32.EnumWindows(enumerate_window, 0)
    return found


def tool_geometry(width, height, dpi):
    scale = min(dpi, width / 620, height / 560)
    return scale, round(44 * scale), round((height - 235 * scale) / 2)


def click_button(hwnd, center_offset):
    bounds = win.RECT()
    if not USER32.GetClientRect(hwnd, ctypes.byref(bounds)):
        raise ctypes.WinError(ctypes.get_last_error())
    dpi = USER32.GetDpiForWindow(hwnd) / 96
    if center_offset == 'wallpapers':
        scale = min(dpi, bounds.right / 2160, bounds.bottom / 560)
        x = round(bounds.right - 174 * scale)
        y = round(bounds.bottom - 80 * scale)
    else:
        scale, left, top = tool_geometry(bounds.right, bounds.bottom, dpi)
        x = round(left + (272 + center_offset) * scale)
        y = round(round(top + 186 * scale) + 49 * scale / 2)
    click_client(hwnd, x, y)


def click_client(hwnd, x, y):
    point, previous = win.POINT(x, y), win.POINT()
    if not (USER32.ClientToScreen(hwnd, ctypes.byref(point)) and
            USER32.GetCursorPos(ctypes.byref(previous))):
        raise ctypes.WinError(ctypes.get_last_error())
    # Winit tracks the physical pointer. A posted click can race WM_MOUSELEAVE
    # if the pointer is outside, so exercise the actual Windows mouse path.
    flags = 0x13  # SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE
    if not USER32.SetWindowPos(hwnd, -1, 0, 0, 0, 0, flags):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        assert USER32.WindowFromPoint(point) == hwnd, "The test window's button is covered"
        # Submit movement and the click together in virtual desktop coordinates.
        # Waiting between them allows unrelated pointer motion to steal the click.
        x = int((point.x - USER32.GetSystemMetrics(76) + 0.5) * 65536 / USER32.GetSystemMetrics(78))
        y = int((point.y - USER32.GetSystemMetrics(77) + 0.5) * 65536 / USER32.GetSystemMetrics(79))
        events = (Input * 3)(Input(0, Mouse(x, y, 0, 0xC001, 0, 0)),
                             Input(0, Mouse(0, 0, 0, 2, 0, 0)),
                             Input(0, Mouse(0, 0, 0, 4, 0, 0)))
        if USER32.SendInput(len(events), events, ctypes.sizeof(Input)) != len(events):
            raise ctypes.WinError(ctypes.get_last_error())
        time.sleep(.08)
    finally:
        current = win.POINT()
        if USER32.GetCursorPos(ctypes.byref(current)) and (current.x, current.y) == (point.x, point.y):
            USER32.SetCursorPos(previous.x, previous.y)
        USER32.SetWindowPos(hwnd, -2, 0, 0, 0, 0, flags)

def click_blur(hwnd):
    click_button(hwnd, 0)


def image_when_ready(path):
    try:
        with Image.open(path) as image:
            return image.convert("RGB")
    except (FileNotFoundError, OSError):
        return None


def evaluate(before, after, moving, scale, *, switch_button=False, starts_blurred=False):
    width, height = before.size
    assert before.size == after.size == moving.size
    scale, glass_x, glass_y = tool_geometry(width, height, scale)
    button_y = round(glass_y + 186 * scale)
    glass_w = round(544 * scale)
    glass_h = round(158 * scale)
    panel = (glass_x + 20, glass_y + 20, glass_x + glass_w - 20, glass_y + glass_h - 20)
    previous, current, animated = (im.crop(panel) for im in (before, after, moving))
    clear, soft = (current, previous) if starts_blurred else (previous, current)

    # Compare the same instant. Remove only the panel and the button, whose
    # label and hover state legitimately change when clicked.
    diff = ImageChops.difference(before, after)
    outside = ImageDraw.Draw(diff)
    outside.rectangle((glass_x - 1, glass_y - 1, glass_x + glass_w + 1, glass_y + glass_h + 1), fill="black")
    buttons_w = 412 if switch_button else 178
    button_x = round(glass_x + (544 - buttons_w) * scale / 2)
    outside.rectangle((button_x - 1, button_y - 1,
                       button_x + round(buttons_w * scale) + 1, button_y + round(49 * scale) + 1), fill="black")
    assert diff.getbbox() is None, f"Blur changed pixels outside the panel and button: {diff.getbbox()}"

    def edges(im):
        edge_map = im.convert("L").filter(ImageFilter.FIND_EDGES)
        return ImageStat.Stat(edge_map.crop((8, 8, im.width - 8, im.height - 8))).mean[0]

    clear_edges = edges(clear)
    assert clear_edges > 0, "No edges in the unblurred test frame"
    ratio = edges(soft) / clear_edges
    motion = ImageStat.Stat(ImageChops.difference(current, animated)).mean
    # The relocated panel can be over a nearly static part of a video. Also
    # check bare wallpaper away from the panel and both animated UI bars.
    wallpaper = (glass_x + glass_w + 2, height // 3, width, height * 2 // 3)
    background_motion = ImageStat.Stat(ImageChops.difference(
        after.crop(wallpaper), moving.crop(wallpaper))).mean
    assert ratio < 0.75, f"Expected blur to remove edges, got ratio {ratio:.3f}"
    assert max(background_motion) > 0.06, f"Wallpaper stopped moving: {background_motion}"
    return {"width": width, "height": height, "blur_edge_ratio": round(ratio, 3),
            "changed_pixels_outside_panel_and_button": 0,
            "animation_mean_rgb_delta": [round(value, 3) for value in motion],
            "wallpaper_mean_rgb_delta": [round(value, 3) for value in background_motion]}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--package", type=Path)
    parser.add_argument("--leave-open", action="store_true")
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    names = ["before.png", "after.png", "animated.png"]
    if any((out / name).exists() for name in names):
        raise RuntimeError("Use a new output directory so old captures cannot pass the check")
    exe = HERE / "target" / "release" / "ika-wgpu-wallpaper-bridge.exe"
    with (out / "window.log").open("w", encoding="utf-8") as log:
        command = [exe, "--proof-dir", out]
        if args.package:
            command.append(args.package.resolve())
        child = subprocess.Popen(command, stdout=log, stderr=log,
                                 creationflags=subprocess.CREATE_NO_WINDOW)
    hwnd = None
    done = False
    try:
        before = wait_for(lambda: image_when_ready(out / names[0]), child)
        owned = windows_of(child.pid)
        assert len(owned) == 1, f"Expected one visible window from this process, found {len(owned)}"
        hwnd = owned[0]
        click_blur(hwnd)
        after = wait_for(lambda: image_when_ready(out / names[1]), child)
        moving = wait_for(lambda: image_when_ready(out / names[2]), child)
        result = {"visible_windows": len(windows_of(child.pid)), "native_mouse_click": True,
                  **evaluate(before, after, moving, USER32.GetDpiForWindow(hwnd) / 96)}
        assert result["visible_windows"] == 1
        print(json.dumps(result, indent=2))
        (out / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        done = True
    finally:
        if not (args.leave_open and done) and child.poll() is None:
            for window in [hwnd] if hwnd else windows_of(child.pid):
                USER32.PostMessageW(window, 0x0010, 0, 0)
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.terminate()
                child.wait(timeout=3)


if __name__ == "__main__":
    main()
