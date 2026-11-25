"""Play official Wallpaper Engine scenes and local MP4s in one wgpu window."""

import argparse
import _winapi
from concurrent.futures import ThreadPoolExecutor
import ctypes as c
from ctypes import wintypes as w
from dataclasses import replace
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

import catalog
from catalog import Media
from signals import Event
from startup_cache import FrameCache


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
WORKSHOP = ROOT / 'media'
CHISA = WORKSHOP / '3608457742'
LYNAE = WORKSHOP / '3635457818'
MP4 = WORKSHOP / '3003524837/Chainsaw Man Girls 1080p.mp4'
DEFAULT_WALLPAPER = HERE / 'default-wallpaper.png'
WE = Path(os.environ.get('ProgramFiles(x86)', r'C:\Program Files (x86)')) / (
    'Steam/steamapps/common/wallpaper_engine'
)
VIEWER = HERE.parent / 'target/release/ika-wgpu-wallpaper-bridge.exe'
DEFAULT_BIN = ROOT / 'scratch/we-bridge-bin'
TRANSFER_BIN = ROOT / 'scratch/we-bridge-transfer-bin'
PARTS = ('host.exe', 'producer.dll', 'video.exe')
VIDEO_PART = 'video-mpv.exe'
# Always the binaries built from this checkout by tools/build.ps1; never guess
# between leftover folders, or a git revert would still run other code.
BIN = DEFAULT_BIN


def transfer_arch():
    """Use zero-copy alias by default; preserve explicit benchmark overrides."""
    return os.environ.get('IKA_TRANSFER_ARCH') or 'direct_alias'


def has_engine(path):
    return all((path / name).is_file() for name in ('wallpaper64.exe', 'version.json')) and all(
        (path / name).is_dir() for name in ('bin', 'assets', 'locale'))


def can_play(item, engine_available):
    return item.playable and (not item.scene or engine_available)


def default_wallpaper():
    return Media('Fondo de Ika', DEFAULT_WALLPAPER, 'image', 'builtin', 'builtin:ika', DEFAULT_WALLPAPER)


U = c.WinDLL('user32', use_last_error=True)
K = c.WinDLL('kernel32', use_last_error=True)
DWM = c.WinDLL('dwmapi')
DWM.DwmGetWindowAttribute.argtypes = [w.HWND, w.DWORD, c.c_void_p, w.DWORD]
DWM.DwmGetWindowAttribute.restype = c.c_long
K.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
K.OpenProcess.restype = w.HANDLE
K.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
K.WaitForSingleObject.restype = w.DWORD
K.CloseHandle.argtypes = [w.HANDLE]
U.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
U.MonitorFromWindow.argtypes = [w.HWND, w.DWORD]
U.MonitorFromWindow.restype = w.HMONITOR
U.IsWindowVisible.argtypes = [w.HWND]
U.GetClientRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
U.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
U.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
U.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
CALLBACK = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
U.EnumWindows.argtypes = [CALLBACK, w.LPARAM]
U.EnumChildWindows.argtypes = [w.HWND, CALLBACK, w.LPARAM]
U.SendMessageTimeoutW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM, w.UINT, w.UINT, c.POINTER(c.c_size_t)]
U.SendMessageTimeoutW.restype = w.LPARAM
U.PostMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]


def link_or_copy(source, destination):
    try:
        os.link(source, destination)
    except OSError:
        shutil.copy2(source, destination)


def stage_engine(installation, stage_dir):
    # Run the signed, unmodified engine executable with disposable local
    # settings. Screensaver preview mode has its own worker process.
    for name in ('wallpaper64.exe', 'version.json'):
        link_or_copy(installation / name, stage_dir / name)
    (stage_dir / 'bin').mkdir()
    for file in (installation / 'bin').iterdir():
        if file.is_file() and (file.suffix.lower() == '.dll' or file.name == 'winrtutil64.exe'):
            link_or_copy(file, stage_dir / 'bin' / file.name)
    # Junctions share installed assets. Pass paths as data to one shell invocation.
    env = dict(os.environ, IKA_BRIDGE_STAGE=str(stage_dir), IKA_BRIDGE_INSTALL=str(installation))
    subprocess.run(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command',
                    "foreach ($name in 'assets', 'locale') { "
                    'New-Item -ItemType Junction -Path (Join-Path $env:IKA_BRIDGE_STAGE $name) '
                    '-Target (Join-Path $env:IKA_BRIDGE_INSTALL $name) -ErrorAction Stop | Out-Null }'],
                   env=env, creationflags=subprocess.CREATE_NO_WINDOW, check=True, timeout=10)


DEFAULT_SCENE_FPS = 60
PREVIEW_HOVER_SECONDS = 0.18


def stage_scene(item, stage_dir, index, overrides, fps=DEFAULT_SCENE_FPS):
    # Keep shader caches local to the disposable install. The project file
    # supplies the actual scene entry inside scene.pkg (scene.json, gifscene.json, etc.).
    scene = stage_dir / f'scene-{index}'
    if not scene.exists():
        source = item.path
        scene.mkdir()
        shutil.copy2(source / 'project.json', scene / 'project.json')
        link_or_copy(item.package, scene / item.package.name)
        if (source / 'shaders/blobsSM40').is_dir():
            shutil.copytree(source / 'shaders/blobsSM40', scene / 'shaders/blobsSM40')
    config = {'layout': 0, 'profile': None, 'selectedwallpapers': {
        f'Monitor{i}': {'file': (scene / 'project.json').as_posix(), 'local': True}
        for i in range(16)}}
    properties = {f'Monitor{i}': {**(item.preset or {}), **overrides} for i in range(16)}
    (stage_dir / 'config.json').write_text(json.dumps({os.getlogin(): {
        'general': {'wallpaperconfig': config,
                    'user': {'hasshownwelcomedialog': True, 'hasshownhighprioritywarning': True,
                             'fps': fps, 'resolution': 'full'},
                    'wallpaperconfigscreensaver': {**config, 'sameaswallpaper': True}},
        'wproperties': {(scene / name).as_posix(): properties for name in ('project.json', item.package.name)},
    }}), encoding='utf-8')


def scene_settings(path):
    try:
        data = json.loads(path.read_text(encoding='utf-8'))
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def windows_on_display(*pids):
    result = []

    @CALLBACK
    def visit(hwnd, _):
        owner = w.DWORD()
        U.GetWindowThreadProcessId(hwnd, c.byref(owner))
        if owner.value in pids and U.IsWindowVisible(hwnd) and U.MonitorFromWindow(hwnd, 0):
            cloaked = w.DWORD()
            if DWM.DwmGetWindowAttribute(hwnd, 14, c.byref(cloaked), c.sizeof(cloaked)) == 0 and cloaked.value:
                return True
            # The Win32 event sink may set WS_VISIBLE but has no GPU window.
            rect = w.RECT()
            U.GetClientRect(hwnd, c.byref(rect))
            if rect.right and rect.bottom:
                result.append(hwnd)
        return True

    U.EnumWindows(visit, 0)
    return result


def stop(child, close_window=False):
    if not child or child.poll() is not None:
        return
    if close_window:
        for hwnd in windows_on_display(child.pid):
            U.PostMessageW(hwnd, 0x10, 0, 0)
        try:
            child.wait(timeout=2)
            return
        except subprocess.TimeoutExpired:
            pass
    child.terminate()
    child.wait(timeout=3)


def wait_for(check, host, viewer=None, timeout=12, wake=None, *, cancel=None):
    limit = time.monotonic() + timeout
    while time.monotonic() < limit:
        if cancel is not None and cancel.is_set():
            raise InterruptedError('Vista previa cancelada')
        if host.poll() is not None:
            raise RuntimeError(f'El motor aislado salió con código {host.returncode}')
        if viewer is not None and viewer.poll() is not None:
            raise RuntimeError(f'La ventana wgpu salió con código {viewer.returncode}')
        if value := check():
            return value
        if wake:
            wake.wait(.04)
        else:
            time.sleep(.04)
    raise TimeoutError('No llegaron cuadros oficiales; revisa el registro del puente')


def prove(host, viewer, folder, producer, soak):
    sys.path.insert(0, str(HERE.parent))
    from verify import click_blur, evaluate, image_when_ready, USER32

    before = wait_for(lambda: image_when_ready(folder / 'before.png'), host, viewer, 35)
    # The only window drawn on a monitor is the winit presentation window.
    on_screen = windows_on_display(host.pid, producer)
    if on_screen:
        details = []
        for hwnd in on_screen:
            rect, cls = w.RECT(), c.create_unicode_buffer(128)
            U.GetWindowRect(hwnd, c.byref(rect))
            U.GetClassNameW(hwnd, cls, len(cls))
            title = c.create_unicode_buffer(512)
            U.GetWindowTextW(hwnd, title, len(title))
            children = []

            @CALLBACK
            def text(child, _):
                data = c.create_unicode_buffer(512)
                size = c.c_size_t()
                U.SendMessageTimeoutW(child, 0xD, len(data), c.addressof(data), 2, 100, c.byref(size))
                if data.value:
                    children.append(data.value)
                return True

            U.EnumChildWindows(hwnd, text, 0)
            details.append((cls.value, title.value, children, rect.left, rect.top, rect.right, rect.bottom))
        raise AssertionError(f'La fuente apareció en un monitor: {details}')
    surfaces = windows_on_display(viewer.pid)
    assert len(surfaces) == 1, f'Esperaba una ventana winit, obtuve {surfaces}'
    hwnd = surfaces[0]
    click_blur(hwnd)
    after = wait_for(lambda: image_when_ready(folder / 'after.png'), host, viewer)
    moving = wait_for(lambda: image_when_ready(folder / 'animated.png'), host, viewer)
    result = {'wallpaper_engine_pid': producer, 'wgpu_pid': viewer.pid,
              'engine_windows_on_monitors': 0, 'wgpu_windows_on_monitors': 1,
              'native_mouse_click': True, **evaluate(before, after, moving, USER32.GetDpiForWindow(hwnd) / 96)}
    if soak:
        def rendered():
            log = (folder / 'viewer.log').read_text(encoding='utf-8')
            return max(map(int, re.findall(r'Shared frames rendered: (\d+)', log)), default=0)

        print(f'Comprobando respuesta y cuadros nuevos durante {soak} s…', flush=True)
        start = time.monotonic()
        first = rendered()
        samples = 0
        while time.monotonic() < start + soak:
            status = c.c_size_t()
            if host.poll() is not None or viewer.poll() is not None or not U.SendMessageTimeoutW(
                    hwnd, 0, 0, 0, 0x22, 700, c.byref(status)):
                raise RuntimeError('La prueba dejó de responder durante la ejecución prolongada')
            samples += 1
            time.sleep(min(1, max(0, start + soak - time.monotonic())))
        frame_delta = rendered() - first
        assert frame_delta >= 60, 'Se dejó de recibir animación desde el motor oficial'
        assert not windows_on_display(host.pid, producer)
        result.update(soak_seconds=soak, responsive_samples=samples, rendered_frame_count_lower_bound=frame_delta)
    (folder / 'result.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2), flush=True)


def read_surface(path):
    try:
        return path.read_text(encoding='ascii')
    except (FileNotFoundError, PermissionError):
        # Windows can briefly deny reads while the producer atomically renames the file.
        return None


class Worker:
    def __init__(self, item, index, generation, runtime, folder, binaries, viewer, scene_720p,
                 settings_dir, scene_fps, *, warm=False, cache_frame=None, cancel=None, cover=None):
        self.process = None
        self.handle = None
        self.audio = self.audio_wake = None
        self.pid = None
        self.runtime = runtime
        self.audio_id = str(generation).removesuffix('-recovered')
        self.settings = None
        self.info = folder / f'surface-{generation}.txt'
        self.engine = item.scene
        env = dict(os.environ, IKA_TRANSFER_ARCH=transfer_arch(), IKA_WE_BRIDGE_INFO=str(self.info),
                   IKA_WE_BRIDGE_LOG=str(folder / f'dxgi-{generation}.log'))
        try:
            if cover is not None:
                start, end = cover
                env.update(IKA_PROFILE_VIDEO_START=format(start, '.6f'),
                           IKA_PROFILE_VIDEO_END=format(end, '.6f'))
            if warm:
                env['IKA_WE_BRIDGE_PREPARE'] = '1'
            if cache_frame is not None:
                env['IKA_WE_BRIDGE_CACHE_FRAME'] = str(cache_frame)
            if cancel is not None:
                env.pop('IKA_WE_BRIDGE_AUDIO', None)
                env.pop('IKA_WE_BRIDGE_AUDIO_WAKE', None)
                if cancel.is_set():
                    raise InterruptedError('Vista previa cancelada')
            if item.kind != 'image' and (cancel is None or item.scene):
                # SceneAudio needs a permanently disabled event to mute an isolated scene.
                self.audio = Event(folder, f'audio-{self.audio_id}', manual=True)
                self.audio_wake = Event(folder, f'audio-wake-{self.audio_id}')
                env.update(IKA_WE_BRIDGE_AUDIO=self.audio.name,
                           IKA_WE_BRIDGE_AUDIO_WAKE=self.audio_wake.name)
            if item.scene:
                self.settings = scene_settings(settings_dir / (catalog.key(item.id) + '.json'))
                stage_scene(item, runtime, index, self.settings, scene_fps)
                command = [binaries / 'host.exe', runtime / 'wallpaper64.exe',
                           binaries / 'producer.dll', runtime]
                if scene_720p:
                    command.append('--scene-720p')
            else:
                video = binaries / (VIDEO_PART if (binaries / VIDEO_PART).is_file() else 'video.exe')
                if cover is not None and (binaries / 'video-profile-mpv.exe').is_file():
                    video = binaries / 'video-profile-mpv.exe'
                command = [video, item.path, str(os.getpid())]
                if item.kind == 'image':
                    command.append('--image')
            with Event(folder, f'frame-{generation}') as first_frame:
                env['IKA_WE_BRIDGE_FRAME_EVENT'] = first_frame.name
                self.log = (folder / f'worker-{generation}.log').open('w', encoding='utf-8', buffering=1)
                startup = None
                parent = None
                if item.scene:
                    # Pass a real process handle. Even if Python exits before
                    # the host starts, the handle still identifies this launch.
                    current = _winapi.GetCurrentProcess()
                    parent = _winapi.DuplicateHandle(current, current, current, 0x100000, True)
                    env['IKA_WE_BRIDGE_PARENT'] = str(parent)
                    startup = subprocess.STARTUPINFO(lpAttributeList={'handle_list': [parent]})
                try:
                    self.process = subprocess.Popen(command, env=env, stdout=self.log, stderr=self.log,
                                                    startupinfo=startup,
                                                    creationflags=subprocess.CREATE_NO_WINDOW)
                finally:
                    if parent is not None:
                        _winapi.CloseHandle(parent)
                metadata = wait_for(lambda: read_surface(self.info), self.process, viewer,
                                    wake=first_frame, cancel=cancel)
            self.pid, *_ = map(int, metadata.split())
            self.handle = K.OpenProcess(0x100000, False, self.pid)
            if not self.handle:
                raise c.WinError(c.get_last_error())
            if item.kind == 'video':
                # Tag the manifest here so older producer binaries work too.
                # Only the launcher publishes the startup/switch notification.
                catalog.atomic(self.info, (metadata.rstrip() + ' video\n').encode('ascii'))
        except BaseException:
            self.close()
            raise

    def close(self):
        try:
            stop(self.process)
        finally:
            if hasattr(self, 'log') and self.log:
                try:
                    self.log.close()
                except OSError:
                    pass
            try:
                if self.handle:
                    handle, self.handle = self.handle, None
                    try:
                        # Stopping the scene host also closes its kill-on-close job.
                        if K.WaitForSingleObject(handle, 3000) != 0:
                            raise RuntimeError(f'Worker {self.pid} did not stop')
                    finally:
                        K.CloseHandle(handle)
            finally:
                for event in (self.audio, self.audio_wake):
                    if event:
                        event.close()


class Previews:
    """Muted preview workers, independent of the applied wallpaper."""

    def __init__(self, playlist, channel):
        self.playlist = playlist
        self.channel = channel
        self.command = None
        self.pending = None
        self.jobs = {}
        self.executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix=f'{channel}-preview')

    def start(self, item, index, token, cancel, cover=None):
        if cancel.is_set():
            return None
        p = self.playlist
        online = isinstance(item, dict)
        isolated = (tempfile.TemporaryDirectory(prefix=f'{self.channel}-', dir=p.runtime)
                    if online or item.scene else None)
        try:
            runtime = Path(isolated.name) if isolated else p.runtime
            if online:
                file = catalog.online_preview(item, runtime, cancel)
                if file is None:
                    isolated.cleanup()
                    return None
                item = Media(item['name'], file, 'video', self.channel, item['id'])
            elif self.channel == 'panel' and item.kind == 'image':
                item = replace(item, kind='video')  # Animated GIFs need MPV, not WIC.
            if item.scene:
                stage_engine(p.engine, runtime)
            worker = Worker(item, index, f'{self.channel}-{token}', runtime, p.folder, p.binaries, p.viewer,
                            p.scene_720p, p.settings_dir, p.scene_fps, warm=True, cancel=cancel, cover=cover)
            return worker, isolated
        except BaseException:
            if isolated:
                isolated.cleanup()
            raise

    @staticmethod
    def stop(future):
        try:
            opened = future.result()
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError):
            return
        if opened:
            worker, isolated = opened
            try:
                worker.close()
            finally:
                if isolated:
                    isolated.cleanup()

    def retire(self, token):
        cancel, future = self.jobs.pop(token)
        cancel.set()
        self.playlist.close_executor.submit(self.stop, future)

    def publish(self, token, kind, index):
        p = self.playlist
        catalog.atomic(p.folder / f'{self.channel}-result-{token}.txt', f'{kind} {index}'.encode('ascii'))
        if p.ready:
            p.ready.set()

    def tick(self):
        p = self.playlist
        try:
            fields = (p.folder / f'{self.channel}-preview.txt').read_text(encoding='ascii').split()
            keep, token, mode = map(int, fields[:3])
            key, = fields[3:]
            base = int(key) if self.channel == 'dock' or mode not in (2, 3) else -1
            command = keep, token, mode, key
        except (OSError, ValueError):
            return
        if command != self.command:
            self.command = command
            for stale in self.jobs.keys() - {keep, token}:
                self.retire(stale)
            self.pending = None
            if token > 0 and token not in self.jobs:
                items = p.items[:]
                cover = None
                if self.channel == 'profile' and mode == 3:
                    index, item = -1, None
                    try:
                        raw = json.loads((p.folder / f'profile-cover-{token}.json').read_text(encoding='utf-8'))
                        path = Path(raw['path']).resolve(strict=True)
                        local = Path(os.environ['LOCALAPPDATA']).resolve()
                        roots = [local / app / 'EBWebView' / 'Default' / 'IndexedDB'
                                 for app in ('com.modcomparer.desktop', 'com.modcomparer.app')]
                        roots.append(local / 'IkaNative' / 'profile-covers')
                        if path.is_file() and any(path.is_relative_to(root) for root in roots):
                            start, duration = float(raw['start']), float(raw['duration'])
                            if 0 <= start <= 86400 and 0.1 <= duration <= 86400:
                                cover = start, start + duration
                                item = Media('Portada', path, 'video', 'profile', str(token))
                    except (OSError, ValueError, KeyError, TypeError):
                        pass
                elif self.channel == 'panel' and mode == 2:
                    index = -1
                    item = next((r for r in p.catalog.online_rows if r.get('poster') == key), None) if p.catalog else None
                else:
                    candidates = ([base] if mode else
                                  [(base + step) % len(items) for step in range(1, len(items))])
                    index = next((i for i in candidates if 0 <= i < len(items) and p.can_play(items[i])
                                  and (self.channel == 'dock' or items[i].animated)), None)
                    item = None if index is None else items[index]
                if item is None:
                    self.publish(token, 'skip', base)
                else:
                    cancel = threading.Event()
                    future = self.executor.submit(self.start, item, index, token, cancel, cover)
                    self.jobs[token] = cancel, future
                    self.pending = token, index
                    if p.request_wake:
                        future.add_done_callback(lambda _: p.request_wake.set())
        if self.pending:
            token, index = self.pending
            _, future = self.jobs[token]
            if future.done():
                self.pending = None
                try:
                    opened = future.result()
                    playing = opened and opened[0].process.poll() is None
                except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
                    print(f'No se pudo preparar la vista previa: {error}', file=sys.stderr, flush=True)
                    playing = False
                self.publish(token, 'worker' if playing else 'skip', index)

    def close(self):
        for token in tuple(self.jobs):
            self.retire(token)
        self.executor.shutdown(wait=True)


class Playlist:
    # Verification subclasses share this API. Test sessions keep their properties
    # beside their disposable scenes.
    settings_dir = None
    # Keep a separate cap for each producer type. Extra workers beyond these
    # mostly contend for shader compilation and hardware decoding.
    preload_scene_workers = min(5, max(1, (os.cpu_count() or 4) // 4))
    preload_media_workers = min(4, max(1, (os.cpu_count() or 4) // 3))

    def __init__(self, items, runtime, folder, binaries, scene_720p, scene_fps=DEFAULT_SCENE_FPS, *,
                 viewer=None, engine=WE, frame_cache=None):
        self.items = items
        self.runtime = runtime
        self.folder = folder
        self.binaries = binaries
        self.engine = engine
        self.engine_available = has_engine(engine)
        self.engine_staged = False
        self.engine_stage_lock = threading.Lock()
        self.scene_720p = scene_720p
        self.scene_fps = scene_fps
        self.generation = 0
        self.index = 0
        self.viewer = viewer
        self.settings_dir = self.settings_dir or runtime
        self.frame_cache = frame_cache
        self.worker = self.start(0, 0)
        self.persist = False
        self.catalog = None
        self.ready = None
        self.request_wake = None
        self.preparing = None
        self.prepared_at = 0
        self.prepare_serial = 0
        self.hover_serial = 0
        self.hover_since = 0
        self.alternate_runtime = None
        self.executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix='wallpaper-preview')
        self.retiring = []
        self.close_executor = ThreadPoolExecutor(max_workers=2, thread_name_prefix='wallpaper-close')
        self.dock_previews = Previews(self, 'dock')
        self.panel_previews = Previews(self, 'panel')
        self.profile_previews = Previews(self, 'profile')

    def start(self, index, generation, *, runtime=None, warm=False):
        runtime = runtime or self.runtime
        if self.items[index].scene and runtime == self.runtime and not self.engine_staged:
            # A video can start without staging the official engine. If the
            # library is already cached, this work can wait for scene playback.
            with self.engine_stage_lock:
                if not self.engine_staged:
                    stage_engine(self.engine, self.runtime)
                    self.engine_staged = True
        file = self.frame_path(index)
        return Worker(self.items[index], index, generation, runtime,
                      self.folder, self.binaries, self.viewer, self.scene_720p, self.settings_dir, self.scene_fps,
                      warm=warm, cache_frame=file if file and not FrameCache.ready(file) else None)

    def can_play(self, item):
        return can_play(item, self.engine_available)

    def frame_path(self, index):
        if self.frame_cache is None:
            return None
        item = self.items[index]
        settings = scene_settings(self.settings_dir / (catalog.key(item.id) + '.json')) if item.scene else None
        return self.frame_cache.path(item, settings)

    def next_index(self):
        return next((i % len(self.items) for i in range(self.index + 1, self.index + len(self.items) + 1)
                     if self.can_play(self.items[i % len(self.items)])), self.index)

    def prepare(self, index, generation, alternate):
        if alternate and self.alternate_runtime is None:
            runtime = Path(tempfile.mkdtemp(prefix='alternate-', dir=self.runtime))
            stage_engine(self.engine, runtime)
            self.alternate_runtime = runtime
        runtime = self.alternate_runtime if alternate else self.runtime
        return self.start(index, generation, runtime=runtime, warm=True)

    def preload_library(self):
        """Ensure active wallpaper is ready; avoid hammering the GPU with parallel workers at startup."""
        total = max(1, len(self.items))
        progress = self.folder / 'preload-progress.txt'
        catalog.atomic(progress, f'{total} {total}'.encode('ascii'))
        catalog.publish(self.folder / 'preload-result.json', dict(
            total=total, reused=total, rendered=0, failures=[]))
        return self.viewer.poll() is None

    def prime(self, index):
        """Keep a downloaded wallpaper's first frame ready for an instant click."""
        if not 0 <= index < len(self.items) or not self.can_play(self.items[index]) or index == self.index:
            return
        if self.preparing:
            if self.preparing[0] == index:
                return
            if not self.preparing[1].done():
                return
            self.take_prepared()
        alternate = self.items[index].scene and self.worker.runtime == self.runtime
        self.prepared_at = time.monotonic()
        future = self.executor.submit(self.prepare, index, f'prime-{index}', alternate)
        self.preparing = index, future

    def take_prepared(self, index=None, timeout=0):
        if not self.preparing:
            return None
        wanted, future = self.preparing
        try:
            worker = future.result(timeout=timeout)
        except TimeoutError:
            # Keep speculative load alive; click may arrive just before its
            # first frame is published.
            return None
        except (OSError, RuntimeError, subprocess.SubprocessError):
            self.preparing = None
            return None  # Speculative load; the normal path can retry on click.
        self.preparing = None
        if wanted == index and worker.process.poll() is None:
            item = self.items[index]
            if not worker.engine or worker.settings == scene_settings(
                    self.settings_dir / (catalog.key(item.id) + '.json')):
                return worker
        worker.close()
        return None

    def poll_prepare(self):
        try:
            serial, choice = (self.folder / 'prepare.txt').read_text(encoding='ascii').split()
            serial, choice = int(serial), int(choice)
        except (OSError, ValueError):
            return
        target = self.next_index() if choice == -1 else choice
        wanted = 0 <= target < len(self.items) and target != self.index and self.can_play(self.items[target])
        now = time.monotonic()
        if serial != self.hover_serial:
            self.hover_serial = serial
            self.hover_since = now
        new = serial > self.prepare_serial
        if self.preparing:
            index, future = self.preparing
            if not future.done():
                return
            # Leave a short grace period when the cursor crosses a card
            # boundary on click. A newer playable target can still replace it.
            if (new and wanted and target != index) or now - self.prepared_at > 4:
                self.take_prepared()
        # Sweep across cards without launching a new GPU worker for each one.
        # Clicks still dispatch immediately through the foreground request.
        if not new or (wanted and now - self.hover_since < PREVIEW_HOVER_SECONDS):
            return
        self.prepare_serial = serial
        if wanted and not self.preparing:
            alternate = self.items[target].scene and self.worker.runtime == self.runtime
            self.prepared_at = now
            future = self.executor.submit(self.prepare, target, f'warm-{serial}', alternate)
            self.preparing = target, future

    def close(self):
        try:
            prepared = self.take_prepared()
            if prepared is not None:
                prepared.close()
        finally:
            self.dock_previews.close()
            self.panel_previews.close()
            self.profile_previews.close()
            self.executor.shutdown(wait=True)
            self.close_executor.shutdown(wait=True)
            self.retiring.clear()
            self.worker.close()

    def retire(self, worker):
        # The old host can remain blocked in its D3D11 Present interception
        # while the already-ready replacement is attached. Do not make the
        # visible switch wait for that teardown.
        self.retiring.append(worker)
        self.close_executor.submit(worker.close)

    def tick(self):
        if self.viewer.poll() is not None:
            return
        self.dock_previews.tick()
        self.panel_previews.tick()
        self.profile_previews.tick()
        generation = self.generation + 1
        request = self.folder / f'request-{generation}'
        if not request.exists():
            if self.worker.process.poll() is not None:
                raise RuntimeError(f'Wallpaper producer exited: {self.worker.process.returncode}')
            if self.catalog:
                self.catalog.tick()
            self.poll_prepare()
            return
        choice = request.read_text(encoding='ascii').strip()
        index = int(choice) if choice else self.next_index()
        if not 0 <= index < len(self.items) or not self.can_play(self.items[index]):
            index = self.index
        message = self.items[index].title
        try:
            if index != self.index and FrameCache.ready(file := self.frame_path(index)):
                catalog.atomic(self.folder / f'preview-{generation}.txt', os.fsencode(file))
                if self.ready:
                    self.ready.set()
        except OSError:
            pass  # The live source is still available if the cache cannot be read.
        try:
            if self.preparing and self.preparing[0] != index and not self.preparing[1].done():
                # Start the clicked item first. The speculative worker may
                # still be writing to the other engine runtime.
                old = self.worker
                self.worker = self.start(index, generation, runtime=old.runtime)
                old.close()
            else:
                # Give an already-started candidate a short chance to publish
                # its first frame before tearing down the visible producer.
                prepared = self.take_prepared(index, timeout=0.25)
                if prepared is not None:
                    old = self.worker
                    self.worker = prepared
                    self.retire(old)
                else:
                    old = self.worker
                    old.close()
                    self.worker = self.start(index, generation)
        except (OSError, TimeoutError, RuntimeError) as error:
            if self.viewer.poll() is not None:
                return
            # Recover the previous source without opening a second window.
            print(f'Could not start {message}: {error}', file=sys.stderr, flush=True)
            self.worker = self.start(self.index, f'{generation}-recovered', runtime=self.worker.runtime)
            message = f'{self.items[self.index].title} (no se pudo cambiar)'
        else:
            self.index = index
        publication = self.folder / f'ready-{generation}.tmp'
        surface = self.folder / f'surface-{generation}.txt'
        if self.worker.info != surface:
            shutil.copyfile(self.worker.info, surface)
        if self.worker.audio and self.worker.audio_id != str(generation):
            (self.folder / f'audio-{generation}.txt').write_text(self.worker.audio_id, encoding='ascii')
        (self.folder / f'active-{generation}.txt').write_text(str(self.index), encoding='ascii')
        publication.write_text(message, encoding='utf-8')
        publication.replace(self.folder / f'ready-{generation}.txt')
        if self.ready:
            self.ready.set()
        self.generation = generation
        request.unlink()
        if self.persist:
            try:
                catalog.atomic(catalog.CACHE / 'active.txt', self.items[self.index].id.encode('ascii'))
            except OSError:
                pass


class CatalogJobs:
    def __init__(self, playlist, engine, scan):
        self.playlist, self.folder, self.engine, self.scan = playlist, playlist.folder, engine, scan
        self.seen, self.jobs = {}, {}
        self.online_rows = []
        self.write_library()
        if self.scan and len(self.playlist.items) <= 1:
            threading.Thread(target=self.rescan, daemon=True, name='catalog-rescan').start()

    def write_library(self):
        catalog.publish(self.folder / 'library.json', [
            dict(item.row(), playable=self.playlist.can_play(item)) for item in self.playlist.items
        ])

    def rescan(self):
        if not self.scan:
            return
        # Existing indices are part of the switch protocol and stay stable during a session.
        items = self.playlist.items
        found = {item.id: item for item in catalog.discover(self.engine)}
        for n, item in enumerate(items):
            if (fresh := found.pop(item.id, None)) is not None:
                items[n] = fresh
        items.extend(found.values())
        self.write_library()

    def start(self, channel, serial, action, data):
        job = self.folder / f'{channel}-{serial}.job'
        result = job.with_suffix('.result')
        catalog.publish(job, [action, data, str(result)])
        child = subprocess.Popen([sys.executable, '-B', catalog.__file__, str(job)],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                 creationflags=subprocess.CREATE_NO_WINDOW)
        self.jobs[channel] = (child, serial, action, job, result)

    def finish(self, channel, result):
        child, serial, action, job, path = self.jobs.pop(channel)
        result['serial'] = serial
        if action == 'online':
            self.online_rows = result.get('rows', [])
            for row in self.online_rows:
                row['poster'] = (catalog.poster_key(row['name'], row['image'])
                                 if row['image'] or catalog.unicode_title(row['name']) else '')
                row['animated'] = 'slug' in row or bool(row['image'])
            catalog.publish(self.folder / 'online.json', result)
        if action in ('download', 'import', 'url'):
            if file := result.get('path'):
                item = Media.from_path(Path(file), 'motion' if action == 'download' else 'imported')
                items = self.playlist.items
                index = next((i for i, entry in enumerate(items) if entry.id == item.id), len(items))
                if index == len(items):
                    items.append(item)
                self.write_library()
                result['index'] = index
                # Start decoding the new item as soon as the download has
                # finished. Keep its first frame alive so selecting it can
                # attach immediately instead of starting cold.
                self.playlist.prime(index)
            catalog.publish(self.folder / 'action.json', result)
        job.unlink(missing_ok=True)
        path.unlink(missing_ok=True)

    def tick(self):
        for channel, (child, _, action, _, result) in list(self.jobs.items()):
            if child.poll() is None:
                continue
            value = json.loads(result.read_text(encoding='utf-8')) if result.is_file() else {}
            if child.returncode and action != 'thumbs':
                value = dict(error='No se pudo completar la operación')
            self.finish(channel, value)
        for channel in ('library', 'properties', 'online', 'thumbs', 'action'):
            # One job per channel. An updated request replaces any queued work.
            if channel in self.jobs:
                continue
            try:
                state = json.loads((self.folder / f'{channel}.request').read_text(encoding='utf-8'))
            except (FileNotFoundError, PermissionError):
                continue
            serial = state['serial']
            if self.seen.get(channel) == serial:
                continue
            self.seen[channel] = serial
            if channel == 'library':
                self.rescan()
                catalog.publish(self.folder / 'library-ready.json', dict(serial=serial))
            elif channel == 'properties':
                self.properties(state)
            elif channel == 'online':
                self.start(channel, serial, 'online', state)
            elif channel == 'thumbs':
                wanted = set(state['posters'][:30])
                paths = [(row['poster'], (row['image'], row['name'], '')) for row in self.online_rows
                         if row['poster'] in wanted]
                for item in self.playlist.items:
                    if (poster := item.poster) and poster in wanted:
                        source = item.poster_input
                        video = str(item.path) if item.kind == 'video' and source != item.path else ''
                        paths.append((poster, (str(source) if source else '', item.name, video)))
                paths = [(id, *data) for id, data in dict(paths).items()
                         if not (catalog.CACHE / catalog.POSTERS / (id + '.png')).is_file()]
                if paths:
                    self.start(channel, serial, 'thumbs', paths)
            elif state.get('kind') == 'import':
                self.start(channel, serial, 'import', None)
            elif state.get('kind') == 'url':
                self.start(channel, serial, 'url', state['url'])
            elif state.get('kind') == 'download':
                row = next((r for r in self.online_rows if 'slug' in r and r['id'] == state['id']), None)
                if row:
                    self.start(channel, serial, 'download', row)
                else:
                    catalog.publish(self.folder / 'action.json', dict(serial=serial, error='Vuelve a buscar este fondo'))
            elif state.get('kind') == 'folder':
                result = dict(serial=serial)
                try:
                    folder = self.playlist.items[self.playlist.index].directory
                    if not folder.is_dir():
                        raise FileNotFoundError(folder)
                    os.startfile(folder)
                except (OSError, IndexError):
                    result['error'] = 'No se pudo abrir la carpeta del fondo'
                catalog.publish(self.folder / 'action.json', result)

    def properties(self, state):
        serial = state['serial']
        try:
            index = state['index']
            item = self.playlist.items[index]
            if not item.scene:
                raise ValueError('Este fondo no es una escena')
            path = self.playlist.settings_dir / (catalog.key(item.id) + '.json')
            controls = []
            values = scene_settings(path)
            changes = state.get('values')
            output = {}
            for key, prop in item.controls().items():
                if not isinstance(prop, dict):
                    continue
                kind = prop.get('type')
                default = (item.preset or {}).get(key, prop.get('value'))
                row = dict(key=key, type=kind, name=catalog.clean(prop.get('text') or key), default=default,
                           value=values.get(key, default), order=prop.get('order', 9999))
                if kind == 'slider':
                    minimum, maximum = prop.get('min', 0), prop.get('max', 100)
                    if type(minimum) not in (int, float) or type(maximum) not in (int, float) or maximum <= minimum:
                        continue
                    step = prop.get('step', 0)
                    row.update(min=minimum, max=maximum, step=step if type(step) in (int, float) and step > 0 else max(1 if type(default) is int else .01, (maximum - minimum) / 100))
                elif kind == 'combo':
                    row['options'] = [dict(label=catalog.clean(x.get('label') or x.get('value')), value=x.get('value'))
                                      for x in prop.get('options', []) if isinstance(x, dict)
                                      and isinstance(x.get('value'), (str, int, float))]
                    if not row['options']:
                        continue
                elif kind != 'bool':
                    continue
                controls.append(row)
                if isinstance(changes, dict) and key in changes:
                    value = changes[key]
                    if (kind == 'bool' and type(value) is bool or
                        kind == 'slider' and type(value) in (int, float) and row['min'] <= value <= row['max'] or
                        kind == 'combo' and any(x['value'] == value for x in row['options'])):
                        if value != default:
                            output[key] = value
                        row['value'] = value
            controls.sort(key=lambda c: c['order'] if type(c['order']) in (int, float) else 9999)
            if changes is not None:
                path.parent.mkdir(exist_ok=True, parents=True)
                catalog.publish(path, output)
            result = dict(rows=controls, index=index, apply=changes is not None)
        except (OSError, KeyError, IndexError, ValueError, TypeError) as error:
            result = dict(error=str(error))
        catalog.publish(self.folder / 'properties.json', dict(serial=serial, **result))

    def close(self):
        for child, _, _, job, result in self.jobs.values():
            stop(child)
            job.unlink(missing_ok=True)
            result.unlink(missing_ok=True)


BRIDGE_SOURCES = ('*.cpp', '*.h', '*.def', 'build.cmd')


def build_all():
    """Compile what changed before opening, like `tauri dev`: the launcher with
    cargo (its progress bar shows here) and the bridge only when its sources are
    newer than its binaries. Returns False when compilation fails."""
    print('Compilando el launcher...', flush=True)
    if subprocess.run(['cargo', 'build', '--release', '--locked'], cwd=ROOT).returncode:
        print('\nLa compilación falló; el launcher no se abrió.'
              '\nSi dice "Acceso denegado", cierra el launcher que está abierto y repite.', flush=True)
        return False
    sources = [path for pattern in BRIDGE_SOURCES for path in HERE.glob(pattern)]
    outputs = [DEFAULT_BIN / part for part in (*PARTS, VIDEO_PART)]
    newest = max(path.stat().st_mtime for path in sources)
    if all(path.is_file() for path in outputs) and min(path.stat().st_mtime for path in outputs) >= newest:
        return True
    print('Compilando el puente de video y Wallpaper Engine...', flush=True)
    script = ROOT / 'tools/build.ps1'
    if subprocess.run(['powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', str(script),
                       '-BridgeOnly'], cwd=ROOT).returncode:
        print('\nLa compilación del puente falló; el launcher no se abrió.', flush=True)
        return False
    return True


def main():
    if sys.stdout is None:
        sys.stdout = open(os.devnull, 'w', encoding='utf-8')
    if sys.stderr is None:
        sys.stderr = open(os.devnull, 'w', encoding='utf-8')
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--engine', type=Path, help='Wallpaper Engine install directory (auto-detected by default)')
    p.add_argument('--chisa', type=Path, default=CHISA)
    p.add_argument('--scene', type=Path, action='append', help='additional Workshop scene directory (repeatable)')
    p.add_argument('--mp4', type=Path, action='append', help='local MP4 to add (repeatable)')
    p.add_argument('--bin', type=Path, default=BIN,
                   help='directory with host.exe, producer.dll, video.exe and optional video-mpv.exe')
    p.add_argument('--viewer', type=Path, default=VIEWER, help='path to the compiled wgpu viewer')
    p.add_argument('--single', action='store_true', help='single Chisa source, without the switch button')
    p.add_argument('--fullscreen', action='store_true',
                   help='open a borderless viewer on the primary monitor')
    p.add_argument('--scene-720p', action='store_true',
                   help='opt in to a wallpaper resolution of up to 720p')
    p.add_argument('--scene-fps', type=int, choices=(30, 60, 120), default=DEFAULT_SCENE_FPS,
                   help=f'Wallpaper Engine scene rate (default: {DEFAULT_SCENE_FPS}); independent of UI animations')
    p.add_argument('--no-trim', action='store_true', help='disable viewer working-set trims at startup and when idle')
    p.add_argument('--verify', action='store_true')
    p.add_argument('--fixtures', action='store_true', help='use the original three verification wallpapers')
    p.add_argument('--no-save', action='store_true', help='do not read or write saved playback preferences')
    p.add_argument('--leave-open', action='store_true')
    p.add_argument('--soak-seconds', type=int, default=0)
    p.add_argument('--no-build', action='store_true', help='open the existing binaries without compiling first')
    args = p.parse_args()
    if not args.no_build and args.viewer == VIEWER and args.bin == BIN and not build_all():
        sys.exit(1)
    viewer_exe = args.viewer.resolve()
    if not viewer_exe.is_file():
        p.error(f'Compila el visor o indica el ejecutable con --viewer: {viewer_exe}')
    if args.soak_seconds < 0 or args.soak_seconds and not args.verify:
        p.error('--soak-seconds requires --verify and a non-negative duration')
    if args.single and (args.scene or args.mp4):
        p.error('--scene and --mp4 require the switch button')
    scan = not (args.verify or args.fixtures or args.single or args.scene or args.mp4)
    engine = args.engine.resolve() if args.engine else next(
        (path for path in (WE, *(root / 'steamapps/common/wallpaper_engine'
                                  for root in catalog.steam_roots(WE))) if has_engine(path)), WE)
    engine_available = has_engine(engine)
    if scan:
        catalog.CACHE.mkdir(parents=True, exist_ok=True)
        saved = catalog.CACHE / 'active.txt'
        chosen = saved.read_text(encoding='ascii').strip() if saved.is_file() and not args.no_save else f'we:{args.chisa.name}'
        first = None
        if args.chisa != CHISA:
            first = Media.from_path(args.chisa)
            if not can_play(first, engine_available):
                first = None
        if not first:
            if chosen.startswith('we:') and chosen[3:].isdecimal():
                target_id = chosen[3:]
                for root in catalog.steam_dirs(engine):
                    p = root / target_id
                    if p.is_dir():
                        try:
                            m = Media.from_path(p)
                            if can_play(m, engine_available):
                                first = m
                                break
                        except (OSError, ValueError):
                            pass
            elif chosen.startswith('file:'):
                for source, root in (('motion', catalog.DOWNLOADS), ('imported', catalog.DOWNLOADS / 'imported')):
                    for path in catalog.entries(root):
                        try:
                            m = Media.from_path(path, source)
                            if m.id == chosen and can_play(m, engine_available):
                                first = m
                                break
                        except (OSError, ValueError):
                            pass
                    if first:
                        break
        if not first:
            items = catalog.discover(engine)
            first = next((item for item in items if item.id == chosen and can_play(item, engine_available)), None)
            first = first or next((item for item in items if can_play(item, engine_available)), None)
            first = first or default_wallpaper()
            items = [first, *(item for item in items if item.id != first.id)]
        else:
            items = [first]
    else:
        require_fixtures = args.verify or args.fixtures or args.single
        if (require_fixtures or args.scene) and not engine_available:
            p.error(f'Las escenas de prueba requieren Wallpaper Engine: {engine}')
        paths = [args.chisa] if require_fixtures or engine_available or args.chisa != CHISA else []
        if not args.single:
            paths.extend(args.scene or ([LYNAE] if require_fixtures or engine_available else []))
            paths.extend(args.mp4 or [MP4])
        items = [Media.from_path(path) for path in paths]
    out_root = ROOT / 'scratch'
    out_root.mkdir(exist_ok=True)
    folder = Path(tempfile.mkdtemp(prefix='we-wgpu-bridge-', dir=out_root))
    print(f'Resultados: {folder}', flush=True)
    persist = scan and not args.no_save
    startup_source = folder / 'startup-surface.txt'

    def launch_viewer(waiting):
        source_arg = '--await-source' if waiting else '--shared-source'
        source = startup_source if waiting else folder / 'surface-0.txt'
        argv = [viewer_exe, source_arg, source, '--title', items[0].title]
        if args.fullscreen:
            argv.append('--fullscreen')
        if not args.single:
            argv.extend(['--switch-dir', folder])
        if not (args.single or args.verify or args.no_trim):
            argv.append('--trim-viewer-once')
        if args.verify:
            argv.extend(['--proof-dir', folder])
        env = dict(os.environ, IKA_TRANSFER_ARCH=transfer_arch(), IKA_WALLPAPER_CACHE=str(catalog.CACHE),
                   IKA_BRIDGE_SCRIPT=str(Path(__file__).resolve()), IKA_BRIDGE_PYTHON=sys.executable)
        if scan or any(item.kind == 'video' for item in items):
            # Permit a shared video frame to copy directly into the surface
            # when it covers the client area. Smaller sources use the light
            # shader path instead.
            env['IKA_PRESENT_COPY'] = '1'
        if persist:
            env['IKA_WALLPAPER_PREFS'] = str(catalog.CACHE / 'playlist.json')
        log = (folder / 'viewer.log').open('w', encoding='utf-8', buffering=1)
        proc = subprocess.Popen(argv, stdout=log, stderr=log, env=env,
                                creationflags=subprocess.CREATE_NO_WINDOW)
        proc._log = log
        return proc

    playlist = viewer = None
    with (tempfile.TemporaryDirectory(prefix='ika-we-isolated-') as temp,
          Event(folder, 'request') as request_event, Event(folder, 'ready') as ready_event):
        runtime = Path(temp)
        try:
            if not args.verify:
                viewer = launch_viewer(waiting=True)
            Playlist.settings_dir = (catalog.CACHE / 'scene-controls') if scan and not args.no_save else None
            binaries = args.bin.resolve()
            frame_cache = None
            if scan:
                try:
                    frame_cache = FrameCache(catalog.CACHE / 'frames-v1', engine, binaries,
                                             args.scene_720p, args.scene_fps)
                except OSError as error:
                    print(f'No se pudo abrir la caché de fotogramas: {error}', file=sys.stderr, flush=True)
            playlist = Playlist(items, runtime, folder, binaries, args.scene_720p, args.scene_fps,
                                viewer=viewer, engine=engine, frame_cache=frame_cache)
            playlist.ready = ready_event
            playlist.request_wake = request_event
            playlist.persist = persist
            if viewer is None:
                viewer = launch_viewer(waiting=False)
            else:
                # Publish the active source without waiting for the catalog.
                if not playlist.preload_library():
                    if viewer.returncode:
                        raise RuntimeError(f'Viewer exited with code {viewer.returncode}')
                    return
                if playlist.worker.process.poll() is not None:
                    raise RuntimeError('El fondo inicial se cerró durante la precarga')
                catalog.atomic(startup_source, playlist.worker.info.read_bytes())
                ready_event.set()
            playlist.viewer = viewer
            playlist.catalog = CatalogJobs(playlist, engine, scan)
            if args.verify:
                if args.single:
                    prove(playlist.worker.process, viewer, folder, playlist.worker.pid, args.soak_seconds)
                else:
                    from switch_verify import prove_switches
                    prove_switches(playlist, args.soak_seconds, fullscreen=args.fullscreen)
                if not args.leave_open:
                    return
            while viewer.poll() is None:
                playlist.tick()
                request_event.wait(.05)
            if viewer.returncode:
                raise RuntimeError(f'Viewer exited with code {viewer.returncode}')
        finally:
            try:
                stop(viewer, close_window=True)
            finally:
                if playlist:
                    try:
                        if playlist.catalog:
                            playlist.catalog.close()
                    finally:
                        playlist.close()


if __name__ == '__main__':
    main()

