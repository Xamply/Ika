"""Native clicks against the real DXGI sources and the one winit window."""

import ctypes as c
import json
import re
import sys
import time

from run import HERE, U, wait_for, windows_on_display

sys.path.insert(0, str(HERE.parent))
from verify import click_button, evaluate, image_when_ready, USER32


def title(hwnd):
    buffer = c.create_unicode_buffer(1024)
    U.GetWindowTextW(hwnd, buffer, len(buffer))
    return buffer.value


def prove_switches(playlist, soak, *, fullscreen=False):
    viewer, folder = playlist.viewer, playlist.folder
    def wait(check, timeout=12):
        return wait_for(check, playlist.worker.process, viewer, timeout)

    def active():
        (hwnd,) = windows_on_display(viewer.pid)
        assert not windows_on_display(playlist.worker.process.pid, playlist.worker.pid)
        assert title(hwnd) == f'Ika wgpu - {playlist.items[playlist.index].title}'
        assert bool(USER32.IsZoomed(hwnd)) != fullscreen, 'Unexpected Win32 startup window state'
        return hwnd

    def rendered_first():
        log = (folder / 'viewer.log').read_text(encoding='utf-8')
        return sum(line == 'Shared frames rendered: 1' for line in log.splitlines())

    result = []
    blur = False
    for step in range(len(playlist.items) + 1):
        switch_ms = None
        if step:
            start = time.monotonic()
            previous = playlist.worker.process
            click_button(hwnd, 95)
            wait(lambda: (folder / f'request-{step}').exists())
            playlist.tick()
            assert previous.poll() is not None
            assert playlist.index == step % len(playlist.items)
            wait(lambda: rendered_first() >= step + 1)
            switch_ms = round((time.monotonic() - start) * 1000)

        proof = folder / str(step)
        before = wait(lambda: image_when_ready(proof / 'before.png'), 35 if step == 0 else 12)
        owned = active()
        if step:
            assert owned == hwnd, 'Switch created another presentation window'
            log = (folder / 'viewer.log').read_text(encoding='utf-8')
            expected = f'Fuente activa: {playlist.items[playlist.index].title} | blur={str(blur).lower()}'
            assert expected in log, 'Switch reset the blur state'
        hwnd = owned
        click_button(hwnd, -117)
        after = wait(lambda: image_when_ready(proof / 'after.png'))
        moving = wait(lambda: image_when_ready(proof / 'animated.png'))
        result.append({
            'source': playlist.items[playlist.index].title,
            'producer_pid': playlist.worker.pid,
            'switch_to_first_present_ms': switch_ms,
            'blur_retained': blur,
            **evaluate(before, after, moving, USER32.GetDpiForWindow(hwnd) / 96,
                       switch_button=True, starts_blurred=blur),
        })
        blur = not blur

    if soak:
        def rendered():
            log = (folder / 'viewer.log').read_text(encoding='utf-8')
            return int(re.findall(r'Shared frames rendered: (\d+)', log)[-1])

        first = rendered()
        deadline = time.monotonic() + soak
        while time.monotonic() < deadline:
            assert U.SendMessageTimeoutW(hwnd, 0, 0, 0, 0x22, 700, c.byref(c.c_size_t()))
            assert viewer.poll() is None and playlist.worker.process.poll() is None
            time.sleep(min(1, max(0, deadline - time.monotonic())))
        assert rendered() > first

    report = {'wgpu_pid': viewer.pid, 'hwnd': hwnd, 'visible_windows': 1,
              'maximized': bool(USER32.IsZoomed(hwnd)),
              'soak_seconds': soak, 'sources': result}
    (folder / 'result.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report, indent=2), flush=True)
