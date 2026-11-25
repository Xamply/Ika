"""Measure installed backgrounds one at a time in a disposable viewer and producer."""

import argparse
import ctypes as c
from ctypes import wintypes as w
import json
import os
from pathlib import Path
import random
import shutil
import statistics
import subprocess
import tempfile
import time

import run
from verify_gpu import GpuEngines, memory, u


CREATION_FLAGS = subprocess.CREATE_NO_WINDOW
BOARD_FIELDS = ('watts', 'gpu_pct', 'decoder_pct', 'memory_controller_pct',
                'sm_mhz', 'vram_mib', 'temperature_c')
BOARD_QUERY = 'power.draw,utilization.gpu,utilization.decoder,utilization.memory,clocks.sm,memory.used,temperature.gpu'
FRAME = 'Shared frames rendered: '
PRESENT = 'Viewer presents/s: '
MODES = {'copy': 'Copy frame bridge selected', 'direct_alias': 'Direct alias bridge selected'}


def board(smi):
    row, = subprocess.check_output(
        [smi, '--query-gpu=' + BOARD_QUERY, '--format=csv,noheader,nounits'],
        creationflags=CREATION_FLAGS, text=True, timeout=3).splitlines()
    return {key: None if value.strip() in ('N/A', '[N/A]') else float(value)
            for key, value in zip(BOARD_FIELDS, row.split(','), strict=True)}


def read_log(path):
    return path.read_text(encoding='utf-8', errors='replace')


def video_info(item, ffprobe):
    if item.kind != 'video' or not ffprobe:
        return {}
    try:
        data = subprocess.check_output(
            [ffprobe, '-v', 'error', '-select_streams', 'v:0', '-show_entries',
             'stream=codec_name,width,height,avg_frame_rate,pix_fmt,bit_rate,duration',
             '-of', 'json', str(item.path)], creationflags=CREATION_FLAGS, timeout=5)
        return json.loads(data)['streams'][0]
    except (OSError, ValueError, IndexError, KeyError, subprocess.SubprocessError) as error:
        return {'error': str(error)}


def summary(values):
    values = [x for x in values if x is not None]
    return {'median': round(statistics.median(values), 2), 'max': round(max(values), 2)} if values else None


def measure(item, index, mode, out, runtime, settings_dir, args, smi, dwm):
    folder = out / f'{index + 1:03d}-{mode}'
    folder.mkdir()
    log = folder / 'viewer.log'
    hwnd_rect = w.RECT()
    worker = viewer = None
    os.environ['IKA_TRANSFER_ARCH'] = mode
    try:
        worker = run.Worker(item, index, 0, runtime, folder, args.bin, None,
                            False, settings_dir, run.DEFAULT_SCENE_FPS)
        source = list(map(int, worker.info.read_text().split()[1:3]))
        if item.scene and MODES[mode] not in read_log(folder / 'dxgi-0.log'):
            raise RuntimeError(f'El productor no confirmó {mode}')
        # Same Home compositor as the launcher. No catalog preloads, second worker,
        # screenshots, timestamp queries or changes to stored playback preferences.
        env = dict(os.environ, IKA_PRESENT_STATS='1', IKA_WALLPAPER_CACHE=str(run.catalog.CACHE))
        for key in ('IKA_COMPARE_HOME_GPU', 'IKA_WALLPAPER_PREFS'):
            env.pop(key, None)
        if args.gpu_test_mode:
            env['IKA_GPU_TEST_MODE'] = args.gpu_test_mode
        else:
            env.pop('IKA_GPU_TEST_MODE', None)
        if args.present_copy:
            env['IKA_PRESENT_COPY'] = '1'
        else:
            env.pop('IKA_PRESENT_COPY', None)
        command = [args.viewer, '--shared-source', worker.info, '--title', item.title, '--trim-viewer-once']
        if args.fullscreen:
            command.append('--fullscreen')
        with log.open('wb') as sink:
            viewer = subprocess.Popen(command, env=env, stdout=sink, stderr=sink, creationflags=CREATION_FLAGS)
        hwnd, = run.wait_for(lambda: run.windows_on_display(viewer.pid), worker.process, viewer, 15)
        if not u.SetWindowPos(hwnd, -1, 0, 0, 0, 0, 0x3):
            raise c.WinError(c.get_last_error())
        run.wait_for(lambda: 'Startup working set trim: done' in read_log(log), worker.process, viewer, 16)
        time.sleep(args.warm)
        if not u.GetClientRect(hwnd, c.byref(hwnd_rect)) or (not args.fullscreen and not u.IsZoomed(hwnd)):
            raise RuntimeError('La ventana de prueba no está maximizada')

        offset = log.stat().st_size
        processes = {'viewer': viewer.pid, 'producer': worker.pid, 'dwm': dwm}
        samples, frames, last = [], [], None
        with GpuEngines() as gpu:
            started = time.monotonic()
            until_sample = started + 1
            with log.open('rb') as source_log:
                source_log.seek(offset)
                while len(samples) < args.samples:
                    if viewer.poll() is not None or worker.process.poll() is not None:
                        raise RuntimeError('El visor o el productor se cerraron durante la medición')
                    now = time.monotonic()
                    for line in source_log.readlines():
                        if line.startswith(FRAME.encode()):
                            last = int(line[len(FRAME):])
                            frames.append((now, last))
                    if now >= until_sample:
                        engines = gpu.read(processes.values())
                        samples.append({
                            'board': board(smi),
                            'engines': None if engines is None else {key: engines[pid] for key, pid in processes.items()},
                            'viewer_private_resident_mib': memory(viewer.pid),
                            'producer_private_resident_mib': memory(worker.pid),
                            'foreground': u.GetForegroundWindow() == hwnd,
                        })
                        until_sample += 1
                    time.sleep(.06)
        with log.open('rb') as stream:
            stream.seek(offset)
            rates = [float(line[len(PRESENT):]) for line in stream if line.startswith(PRESENT.encode())]
        if not rates or min(rates) < 1:
            raise RuntimeError('El visor no mantuvo una ventana activa durante la medición')
        if not any(row['engines'] is not None for row in samples):
            raise RuntimeError('Windows no entregó muestras válidas de los motores GPU')
        metrics = {key: summary(row['board'][key] for row in samples) for key in BOARD_FIELDS}
        for process in processes:
            for engine in ('3D', 'Copy', 'VideoDecode', 'VideoProcessing', 'Compute'):
                metrics[f'{process}_{engine}'] = summary(
                    row['engines'][process].get(engine, 0) for row in samples if row['engines'] is not None)
        for process in ('viewer', 'producer'):
            key = f'{process}_private_resident_mib'
            metrics[key] = summary(row[key] for row in samples)
        source_rate = ((last - frames[0][1]) / (frames[-1][0] - frames[0][0])
                       if len(frames) >= 2 and frames[-1][0] > frames[0][0] else None)
        return {
            'source_px': source, 'window_px': [hwnd_rect.right, hwnd_rect.bottom],
            'viewer_fps': summary(rates), 'viewer_fps_min': min(rates),
            'source_fps_estimate': None if source_rate is None else round(source_rate, 1),
            'foreground_samples': sum(row['foreground'] for row in samples),
            'metrics': metrics, 'samples': samples, 'log_directory': folder.name,
        }
    finally:
        try:
            run.stop(viewer, close_window=True)
        finally:
            if worker:
                worker.close()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--bin', type=Path, default=run.BIN)
    p.add_argument('--viewer', type=Path, default=run.VIEWER)
    p.add_argument('--engine', type=Path, default=run.WE)
    p.add_argument('--samples', type=int, default=5)
    p.add_argument('--warm', type=float, default=1.5)
    p.add_argument('--gpu-test-mode', choices=('background',),
                   help='Aísla composición de fondo en el visor; conserva el productor real.')
    p.add_argument('--present-copy', action='store_true',
                   help='Prueba presentar una textura del mismo tamaño con CopyResource.')
    p.add_argument('--fullscreen', action='store_true',
                   help='Mide el visor en monitor completo para probar coincidencia exacta de tamaño.')
    p.add_argument('--only', nargs='+', help='Optional IDs for a focused rerun')
    args = p.parse_args()
    if args.samples < 3 or args.warm < 0:
        p.error('Usa al menos tres muestras y una estabilización no negativa')
    args.bin, args.viewer, args.engine = (path.resolve() for path in (args.bin, args.viewer, args.engine))
    if args.gpu_test_mode:
        os.environ['IKA_GPU_TEST_MODE'] = args.gpu_test_mode
    if args.present_copy:
        os.environ['IKA_PRESENT_COPY'] = '1'
    if not args.viewer.is_file() or not all((args.bin / part).is_file() for part in run.PARTS):
        p.error('Faltan el visor o los binarios del puente')
    smi, ffprobe = shutil.which('nvidia-smi'), shutil.which('ffprobe')
    if not smi:
        p.error('nvidia-smi es necesario para medir toda la tarjeta')
    dwm = int(subprocess.check_output(
        ['powershell.exe', '-NoProfile', '-NonInteractive', '-Command',
         '(Get-Process dwm | Where-Object SessionId -eq (Get-Process -Id $PID).SessionId).Id'],
        creationflags=CREATION_FLAGS, text=True, timeout=5).strip())
    u.GetForegroundWindow.restype = w.HWND
    items = [item for item in run.catalog.discover(args.engine) if not args.only or item.id in args.only]
    if not items:
        p.error('No se encontraron fondos')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = dict(
        binaries=str(args.bin), viewer=str(args.viewer), sample_count=args.samples,
        warm_seconds=args.warm, scene_target_fps=run.DEFAULT_SCENE_FPS,
        adapter=subprocess.check_output([smi, '--query-gpu=name,driver_version',
                                        '--format=csv,noheader'], text=True, creationflags=CREATION_FLAGS).strip(),
        desktop_before=[board(smi) for _ in range(3)],
        desktop_after=[], complete=False,
        items=[dict(id=item.id, name=item.name, kind=item.kind, source=item.source,
                    playable=item.playable, runs=[]) for item in items],
    )

    def save():
        run.catalog.atomic(out / 'results.json', json.dumps(report, ensure_ascii=False, indent=2).encode('utf-8'))

    save()
    # Mix heavy and light content; alternate A/B order to limit warm-device bias.
    ordering = [i for i, item in enumerate(items) if item.playable]
    random.Random(4070).shuffle(ordering)
    with tempfile.TemporaryDirectory(prefix='runtime-', dir=out) as temp:
        runtime = Path(temp)
        if any(item.scene for item in items):
            run.stage_engine(args.engine, runtime)
        settings = run.catalog.CACHE / 'scene-controls'
        for seq, index in enumerate(ordering):
            item, row = items[index], report['items'][index]
            row['media'] = video_info(item, ffprobe)
            modes = tuple(MODES) if item.scene else ('media',)
            if seq % 2:
                modes = modes[::-1]
            for mode in modes:
                try:
                    result = measure(item, index, mode, out, runtime, settings, args, smi, dwm)
                except (OSError, RuntimeError, ValueError, AssertionError, subprocess.SubprocessError) as error:
                    result = {'error': f'{type(error).__name__}: {error}', 'log_directory': f'{index + 1:03d}-{mode}'}
                row['runs'].append(dict(mode=mode, **result))
                save()
                metrics = result.get('metrics')
                status = (f"{metrics['gpu_pct']['median']}% / {metrics['watts']['median']} W"
                          if metrics else result['error'])
                print(f'[{seq + 1}/{len(ordering)}] {item.name} ({mode}): {status}', flush=True)
    report['desktop_after'] = [board(smi) for _ in range(3)]
    report['complete'] = True
    save()


if __name__ == '__main__':
    main()
