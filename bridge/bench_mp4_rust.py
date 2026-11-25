"""Measure mp4_player, its decoder and the visible Rust viewer on Windows."""

import argparse
import ctypes as c
from ctypes import wintypes as w
from fractions import Fraction
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time

import run
from verify_gpu_catalog import board, GpuEngines, memory, summary


def sample_video(args, clip, folder):
    folder.mkdir(exist_ok=False)
    info = folder / 'surface.txt'
    env = {k: v for k, v in os.environ.items() if not k.startswith(('IKA_', 'MPV_'))}
    env.update(IKA_WE_BRIDGE_INFO=str(info), IKA_WE_BRIDGE_LOG=str(folder / 'decoder.log'),
               IKA_MPV_DLL=str(args.worker.with_name('libmpv-2.dll')), IKA_PRESENT_STATS='1',
               IKA_TRANSFER_ARCH='direct_alias')
    if args.background:
        env['IKA_GPU_TEST_MODE'] = 'background'
    if args.present_copy:
        env['IKA_PRESENT_COPY'] = '1'
    probe = subprocess.check_output(
        [args.ffprobe, '-v', 'error', '-select_streams', 'v:0', '-show_entries',
         'stream=codec_name,pix_fmt,width,height,avg_frame_rate,duration', '-of', 'json', str(clip)],
        timeout=10)
    media = json.loads(probe)['streams'][0]
    expected = float(Fraction(media['avg_frame_rate']))
    if float(media['duration']) < args.seconds + 5:
        raise ValueError(f'{clip.name}: usa un clip de al menos {args.seconds + 5} segundos')
    player = viewer = None
    log = folder / 'viewer.log'
    try:
        with (folder / 'player.log').open('wb') as sink:
            player = subprocess.Popen([args.player, clip, args.worker], env=env, stdout=sink, stderr=sink)
        worker, width, height, *_ = map(int, run.wait_for(lambda: run.read_surface(info), player, timeout=15).split())
        if [width, height] != [media['width'], media['height']]:
            raise RuntimeError(f'Salida {width}x{height} no coincide con el MP4')
        command = [args.viewer, '--shared-source', info, '--fullscreen', '--trim-viewer-once']
        with log.open('wb') as sink:
            viewer = subprocess.Popen(command, env=env, stdout=sink, stderr=sink)
        u = run.U
        u.IsIconic.argtypes = [w.HWND]
        u.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
        def visible_window():
            for hwnd in run.windows_on_display(viewer.pid):
                if u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x13):
                    return hwnd
            return None
        hwnd = run.wait_for(visible_window, player, viewer, timeout=15)
        rect = w.RECT()
        if not u.GetClientRect(hwnd, c.byref(rect)):
            raise c.WinError()
        pids = {'player': player.pid, 'decoder': worker, 'viewer': viewer.pid}
        samples, frames, rates = [], [], []
        with GpuEngines() as gpu, log.open('rb') as playback:
            def drain():
                for line in playback.readlines():
                    now = time.monotonic()
                    if m := re.match(rb'Shared frames rendered: (\d+)', line):
                        frames.append((now, int(m[1])))
                    if m := re.match(rb'Viewer presents/s: (\d+(?:\.\d+)?)', line):
                        rates.append((now, float(m[1])))

            first = time.monotonic()
            while len(frames) < 2:
                drain()
                if player.poll() is not None or viewer.poll() is not None or time.monotonic() - first > 15:
                    raise RuntimeError(f'No avanzan los cuadros del visor; revisa {log}')
                time.sleep(.02)
            start = time.monotonic()
            due = start + 1
            while len(samples) < args.seconds:
                if player.poll() is not None or viewer.poll() is not None or u.IsIconic(hwnd):
                    raise RuntimeError('La reproducción dejó de estar visible')
                drain()
                if time.monotonic() >= due:
                    engines = gpu.read(pids.values())
                    samples.append({
                        'engines': None if engines is None else {key: engines[pid] for key, pid in pids.items()},
                        'ram_private_mib': {key: memory(pid) for key, pid in pids.items()},
                        'board': board(args.smi),
                    })
                    due += 1
                time.sleep(.02)
            drain()
        valid = [s['engines'] for s in samples if s['engines']]
        if not any(s['decoder'] for s in valid) or not any(s['viewer'] for s in valid):
            raise RuntimeError('Sin contadores GPU del decodificador o del visor')
        stamps = [(t, n) for t, n in frames if t >= start]
        if len(stamps) < 2:
            raise RuntimeError(f'Muestras insuficientes para medir FPS: {stamps}')
        fps = (stamps[-1][1] - stamps[0][1]) / (stamps[-1][0] - stamps[0][0])
        presents = [v for t, v in rates if t >= start + 1]
        decode = summary(s['decoder'].get('VideoDecode', 0) for s in valid)
        return dict(file=str(clip), source=media, surface_px=[width, height],
                    viewport_px=[rect.right, rect.bottom], delivered_fps=round(fps, 2),
                    presents_fps=summary(presents),
                    native_cadence=abs(fps - expected) <= expected * .03,
                    ram_private_mib=summary(sum(s['ram_private_mib'].values()) for s in samples),
                    ram_by_process_mib={key: summary(s['ram_private_mib'][key] for s in samples) for key in pids},
                    gpu_player_pct={key: summary(max(s[key].values(), default=0) for s in valid) for key in pids},
                    video_decode_pct=decode,
                    viewer_3d_pct=summary(s['viewer'].get('3D', 0) for s in valid),
                    board_gpu_pct=summary(s['board']['gpu_pct'] for s in samples), samples=samples)
    finally:
        run.stop(viewer, close_window=True)
        run.stop(player)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for option in ('player', 'worker', 'viewer', 'output'):
        p.add_argument('--' + option, type=lambda v: Path(v).resolve(), required=True)
    p.add_argument('--ffprobe', default='ffprobe')
    p.add_argument('--smi', default='nvidia-smi')
    p.add_argument('--seconds', type=int, default=10)
    p.add_argument('--background', action='store_true')
    p.add_argument('--present-copy', action='store_true')
    p.add_argument('clips', type=lambda v: Path(v).resolve(), nargs='+')
    args = p.parse_args()
    if args.seconds < 6:
        p.error('Se necesitan al menos seis muestras de un segundo')
    for path in (args.player, args.worker, args.viewer, args.worker.with_name('libmpv-2.dll'), *args.clips):
        if not path.is_file():
            p.error(f'No existe {path}')
    args.output.mkdir(parents=True, exist_ok=False)
    report = dict(adapter=subprocess.check_output(
        [args.smi, '--query-gpu=name,driver_version', '--format=csv,noheader'], text=True, timeout=5).strip(),
        desktop_before=[board(args.smi) for _ in range(3)], seconds=args.seconds,
        background=args.background, present_copy=args.present_copy, runs=[])
    failed = False
    for index, clip in enumerate(args.clips):
        try:
            result = sample_video(args, clip, args.output / f'{index + 1}-{clip.stem}')
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as exc:
            result = dict(file=str(clip), error=str(exc))
            failed = True
        report['runs'].append(result)
        (args.output / 'results.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
        print(json.dumps({k: v for k, v in result.items() if k != 'samples'}), flush=True)
    return int(failed or any(not r.get('native_cadence') for r in report['runs']))


if __name__ == '__main__':
    sys.exit(main())
