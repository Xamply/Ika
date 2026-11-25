"""Sample the viewer and its producer with Windows GPU-engine counters."""

import ctypes as c
from ctypes import wintypes as w
import json
import os
import shutil
import statistics
import subprocess
import sys
import time

import run
from verify_catalog import memory
from verify import USER32 as u, click_button, click_client


class CounterValue(c.Structure):
    _fields_ = [('status', w.DWORD), ('value', c.c_double)]


class CounterItem(c.Structure):
    _fields_ = [('name', w.LPWSTR), ('data', CounterValue)]


class GpuEngines:
    def __enter__(self):
        self.pdh = p = c.WinDLL('pdh')
        p.PdhOpenQueryW.argtypes = [w.LPCWSTR, c.c_size_t, c.POINTER(w.HANDLE)]
        p.PdhAddEnglishCounterW.argtypes = [w.HANDLE, w.LPCWSTR, c.c_size_t, c.POINTER(w.HANDLE)]
        p.PdhCollectQueryData.argtypes = [w.HANDLE]
        p.PdhGetFormattedCounterArrayW.argtypes = [
            w.HANDLE, w.DWORD, c.POINTER(w.DWORD), c.POINTER(w.DWORD), c.c_void_p,
        ]
        p.PdhGetFormattedCounterArrayW.restype = w.DWORD
        p.PdhCloseQuery.argtypes = [w.HANDLE]
        self.query, self.counter = w.HANDLE(), w.HANDLE()
        assertp(p.PdhOpenQueryW(None, 0, c.byref(self.query)))
        try:
            assertp(p.PdhAddEnglishCounterW(self.query, r'\GPU Engine(*)\Utilization Percentage',
                                           0, c.byref(self.counter)))
            assertp(p.PdhCollectQueryData(self.query))
        except BaseException:
            p.PdhCloseQuery(self.query)
            raise
        return self

    def __exit__(self, *_):
        self.pdh.PdhCloseQuery(self.query)

    def read(self, pid):
        p = self.pdh
        assertp(p.PdhCollectQueryData(self.query))
        size, count = w.DWORD(), w.DWORD()
        args = (self.counter, 0x200, c.byref(size), c.byref(count))
        status = p.PdhGetFormattedCounterArrayW(*args, None)
        if status == 0xc0000bba:  # GPU counter instance list changed while sampling.
            return None
        assert status == 0x800007d2
        buffer = c.create_string_buffer(size.value)
        status = p.PdhGetFormattedCounterArrayW(*args, buffer)
        if status in (0x800007d2, 0xc0000bba):
            return None
        assertp(status)
        instances = c.cast(buffer, c.POINTER(CounterItem))
        engines = {process: {} for process in pid}
        for i in range(count.value):
            item, value = instances[i].name, instances[i].data
            if value.status not in (0, 1) or not item.startswith('pid_'):
                continue
            process = int(item.split('_', 2)[1])
            if process in engines:
                # Task Manager uses the busiest engine for a process, not the sum.
                kind = item.split('_engtype_', 1)[-1]
                engines[process][kind] = max(value.value, engines[process].get(kind, 0))
        return engines


def assertp(status):
    if status:
        raise RuntimeError(f'PDH error: 0x{status & 0xffffffff:08x}')


class Check(run.Playlist):
    def tick(self):
        log = self.folder / 'viewer.log'
        start = time.monotonic()
        while 'Startup working set trim: done' not in log.read_text(encoding='utf-8'):
            assert self.viewer.poll() is None and time.monotonic() - start < 60, log
            super().tick()
            time.sleep(.1)
        hwnd, = run.windows_on_display(self.viewer.pid)
        rect = w.RECT()
        assert u.GetClientRect(hwnd, c.byref(rect)) and u.IsZoomed(hwnd)
        u.GetForegroundWindow.restype = w.HWND
        u.ShowWindow.argtypes = [w.HWND, c.c_int]
        u.IsIconic.argtypes = [w.HWND]
        ids = self.viewer.pid, self.worker.pid
        # DWM can be on other sessions as well. Include only this desktop.
        dwm = int(subprocess.check_output(
            ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command',
             '(Get-Process dwm | Where-Object SessionId -eq (Get-Process -Id $PID).SessionId).Id'],
            creationflags=subprocess.CREATE_NO_WINDOW, text=True, timeout=5,
        ).strip() or '0')
        smi = shutil.which('nvidia-smi') if '--gpu-check-power' in options else None
        if '--gpu-check-power' in options and not smi:
            raise RuntimeError('nvidia-smi no está disponible')

        def frame():
            return int(next(line.split()[-1] for line in reversed(log.read_text(encoding='utf-8').splitlines())
                            if line.startswith('Shared frames rendered:')))

        def sample(moving=True):
            # Sample after startup and any CRT transition; use the same warmup in A and B.
            time.sleep(1.5)
            log_offset = log.stat().st_size
            samples = []
            board = []
            frame_times = []
            with GpuEngines() as gpu:
                previous = frame()
                for _ in range(8):
                    end = time.monotonic() + 1
                    while time.monotonic() < end:
                        time.sleep(.04)
                        current = frame()
                        if current != previous:
                            assert current > previous, 'The sample changed background'
                            previous = current
                            frame_times.append((time.monotonic(), current))
                        super(Check, self).tick()
                    engines = gpu.read((*ids, dwm))
                    if engines is not None and (not moving or '3D' in engines[ids[0]]):
                        samples.append((engines[ids[0]].get('3D', 0), engines[ids[1]].get('3D', 0), memory(ids[0]),
                                        engines[dwm].get('3D', 0), engines[ids[0]], engines[ids[1]]))
                    if smi:
                        row, = subprocess.check_output(
                            [smi, '--query-gpu=power.draw,utilization.gpu,clocks.sm', '--format=csv,noheader,nounits'],
                            text=True, creationflags=subprocess.CREATE_NO_WINDOW, timeout=3).splitlines()
                        board.append(tuple(map(float, row.split(','))))
            assert len(samples) >= 4
            assert self.viewer.poll() is None and self.worker.process.poll() is None
            assert self.worker.pid == ids[1], 'The sample changed producer'
            if moving:
                assert len(frame_times) >= 2
                first, last = frame_times[0], frame_times[-1]
                fps = (last[1] - first[1]) / (last[0] - first[0])
            else:
                assert not frame_times and u.IsIconic(hwnd), 'The minimized viewer kept rendering'
                fps = 0.0
            result = {
                'foreground': u.GetForegroundWindow() == hwnd,
                'viewer_3d_pct_median': round(statistics.median(x[0] for x in samples), 2),
                'producer_3d_pct_median': round(statistics.median(x[1] for x in samples), 2),
                'dwm_3d_pct_median': round(statistics.median(x[3] for x in samples), 2),
                'viewer_private_ws_mib_median': round(statistics.median(x[2] for x in samples), 2),
                'viewer_private_ws_mib_max': round(max(x[2] for x in samples), 2),
                'fps_estimate': round(fps, 1),
            }
            for index, prefix in ((0, 'viewer'), (1, 'producer')):
                for kind in ('Copy', 'VideoDecode', 'VideoProcessing'):
                    values = [sample[4 + index].get(kind, 0) for sample in samples]
                    result[f'{prefix}_{kind.lower()}_pct_median'] = round(statistics.median(values), 2)
            with log.open('rb') as stream:
                stream.seek(log_offset)
                lines = stream.readlines()
            rates = [float(line.split()[-1]) for line in lines if line.startswith(b'Viewer presents/s:')]
            if moving and rates:
                result['viewer_presents_per_s_median'] = round(statistics.median(rates), 1)
            for path in ('reference', 'home'):
                times = [float(line.split()[-1]) for line in lines if line.startswith(f'GPU pass us {path} '.encode())]
                if times:
                    result[f'{path}_gpu_us_median'] = round(statistics.median(times), 2)
            if board:
                result.update(zip(('board_w_median', 'board_gpu_pct_median', 'sm_mhz_median'),
                                  (round(statistics.median(col), 2) for col in zip(*board))))
            return result

        # Start with the same foreground state as after the panel clicks.
        click_client(hwnd, rect.right * 3 // 4, rect.bottom // 2)
        assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
        result = {
            'scene': self.items[self.index].name,
            'source_px': list(map(int, self.worker.info.read_text().split()[1:3])),
            'window_px': [rect.right, rect.bottom],
            'launcher': sample(),
        }
        if '--gpu-check-panels' in options:
            click_button(hwnd, -117)
            run.wait_for(lambda: 'Blur: ON' in log.read_text(encoding='utf-8'),
                         self.worker.process, self.viewer)
            assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
            result['blur'] = sample()
            click_button(hwnd, -117)
            run.wait_for(lambda: 'Blur: OFF' in log.read_text(encoding='utf-8'),
                         self.worker.process, self.viewer)
        if options & {'--gpu-check-panels', '--gpu-check-catalog'}:
            # Keep the real pointer off cards so the benchmark never primes a
            # second scene while timing the settled catalog.
            neutral = w.POINT(rect.right // 10, rect.bottom // 2)
            if u.ClientToScreen(hwnd, c.byref(neutral)):
                u.SetCursorPos(neutral.x, neutral.y)
            click_button(hwnd, 'wallpapers')
            run.wait_for((self.folder / 'thumbs.request').is_file, self.worker.process, self.viewer)
            assert u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13)
            result['catalog'] = sample()
        if '--gpu-check-minimize' in options:
            u.ShowWindow(hwnd, 6)  # SW_MINIMIZE
            run.wait_for(lambda: u.IsIconic(hwnd), self.worker.process, self.viewer)
            result['minimized'] = sample(moving=False)
            u.ShowWindow(hwnd, 9)  # SW_RESTORE
            run.wait_for(lambda: not u.IsIconic(hwnd), self.worker.process, self.viewer)
            assert u.IsZoomed(hwnd)
            click_client(hwnd, rect.right * 3 // 4, rect.bottom // 2)
            result['restored'] = sample()
        (self.folder / 'gpu-proof.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2), flush=True)
        raise SystemExit(0)


if __name__ == '__main__':
    os.environ['IKA_PRESENT_STATS'] = '1'
    options = set(sys.argv) & {
        '--gpu-check-panels', '--gpu-check-catalog', '--gpu-check-minimize', '--gpu-check-power', '--gpu-check-renderer',
    }
    if '--gpu-check-renderer' in options:
        os.environ['IKA_COMPARE_HOME_GPU'] = '1'
    sys.argv = [arg for arg in sys.argv if arg not in options] + ['--no-save']
    run.Playlist = Check
    run.main()
