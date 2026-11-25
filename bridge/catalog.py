"""Disk library and on-demand Wallpaper Engine / MotionBGs catalog jobs."""

from dataclasses import dataclass, replace
from hashlib import sha256
from html import unescape
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
from urllib.parse import parse_qsl, unquote, urlencode, urlparse


ROOT = Path(__file__).resolve().parents[1]
DOWNLOADS = Path(os.environ.get('USERPROFILE', str(Path.home()))) / 'Downloads/ModComparer_Wallpapers'
CACHE = Path(os.environ.get('LOCALAPPDATA', tempfile.gettempdir())) / 'IkaNative/wallpapers'
POSTERS = 'posters-hd'
POSTER_SIZE = (464, 320)  # 2x the card's logical size.
MOTION = ('minecraft', 'anime', 'nature', 'cyberpunk', 'space', 'pixel-art',
          'fantasy', 'gaming', 'relaxing', 'sunset')
STEAM = ('', 'minecraft', 'Anime', 'Landscape', 'Pixel art', 'Cyberpunk',
         'Relaxing', 'Fantasy', 'Sci-Fi', 'Game')
SORTS = ('toprated', 'trend', 'totaluniquesubscribers', 'mostrecent')
IMAGES = {'.jpg', '.jpeg', '.png', '.bmp', '.gif', '.webp', '.avif', '.tif', '.tiff'}
VIDEO = {'.mp4', '.webm', '.m4v', '.mov', '.wmv', '.mkv', '.avi'}
AGENT = ('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 '
         '(KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36')
CREATE_NO_WINDOW = getattr(subprocess, 'CREATE_NO_WINDOW', 0)


def key(value):
    return sha256(str(value).encode('utf-8')).hexdigest()[:24]


def atomic(path, data):
    tmp = path.with_suffix(path.suffix + '.part')
    tmp.write_bytes(data)
    # Windows can deny replacement while a reader briefly holds the target.
    for attempt in range(6):
        try:
            tmp.replace(path)
            return
        except PermissionError as error:
            if getattr(error, 'winerror', None) not in (5, 32) or attempt == 5:
                raise
            time.sleep(0.005 * (attempt + 1))


def publish(path, data):
    atomic(path, json.dumps(data, ensure_ascii=False).encode('utf-8'))


def clean(text):
    return ' '.join(unescape(str(text)).split())[:240]


def unicode_title(name):
    return any(ord(ch) > 255 for ch in name)


def poster_key(name, *source):
    parts = source + (name,)
    if unicode_title(name):
        parts += ('title-mask-v2',)
    return key(parts if len(parts) != 1 else name)


def entries(path):
    try:
        return sorted(path.iterdir())
    except OSError:
        return []


def child(folder, rel):
    if not rel or not isinstance(rel, str):
        return None
    try:
        path = (folder / rel).resolve()
        return path if path.is_relative_to(folder.resolve()) and path.is_file() else None
    except (OSError, ValueError):
        return None


@dataclass(frozen=True)
class Media:
    name: str
    path: Path
    kind: str
    source: str
    id: str
    preview: Path | None = None
    package: Path | None = None
    tags: tuple = ()
    author: str = ''
    dependency: str = ''
    preset: dict | None = None
    project_dir: Path | None = None

    @property
    def directory(self):
        return self.project_dir or self.path.parent

    @property
    def scene(self):
        return self.kind == 'scene'

    @property
    def playable(self):
        return self.kind in ('scene', 'video', 'image')

    @property
    def animated(self):
        return self.kind in ('video', 'scene') or self.path.suffix.lower() == '.gif'

    @property
    def title(self):
        label = 'Wallpaper Engine' if self.scene else ('Imagen' if self.kind == 'image' else self.path.suffix[1:].upper())
        return f'{label} | {self.name}'

    @property
    def poster_input(self):
        return self.preview or (self.path if self.kind == 'video' else None)

    @property
    def poster(self):
        if source := self.poster_input:
            try:
                parts = (source, source.stat().st_mtime_ns)
                if self.kind == 'video' and source != self.path:
                    parts += (self.path, self.path.stat().st_mtime_ns)
                return poster_key(self.name, *parts)
            except OSError:
                pass
        return poster_key(self.name) if unicode_title(self.name) else ''

    def row(self):
        res = re.search(r'(4k|1440p|1080p)$', self.path.stem, re.I) if self.source == 'motion' else None
        media = self.package or self.path
        try:
            size = media.stat().st_size if media.is_file() else None
        except OSError:
            size = None
        return dict(name=self.name, kind=self.kind, source=self.source, id=self.id,
                    poster=self.poster, tags=self.tags, author=self.author,
                    size_bytes=size, resolution=res.group() if res else '', animated=self.animated)

    def controls(self):
        if not self.scene:
            return {}
        project = json.loads((self.path / 'project.json').read_text(encoding='utf-8-sig'))
        props = project.get('general', {}).get('properties', {})
        return props if isinstance(props, dict) else {}

    @classmethod
    def from_path(cls, path, source='imported'):
        path = path.resolve(strict=True)
        if path.is_dir():
            project = child(path, 'project.json')
            meta = json.loads(project.read_text(encoding='utf-8-sig')) if project else {}
            name = clean(meta.get('title') or path.name)
            typ = str(meta.get('type', '')).lower()
            preview = child(path, meta.get('preview'))
            if not preview or preview.suffix.lower() not in IMAGES:
                preview = next((p for p in entries(path)
                                if p.is_file() and p.suffix.lower() in IMAGES
                                and p.stem.lower().startswith('preview')), None)
            target = child(path, meta.get('file'))
            package = child(path, 'scene.pkg')
            if not package and typ == 'scene' and isinstance(meta.get('file'), str):
                package = child(path, str(Path(meta['file']).with_suffix('.pkg')))
            if not package and typ == 'scene':
                package = next((p for p in entries(path) if p.suffix.lower() == '.pkg' and p.is_file()), None)
            if typ == 'web' or meta.get('dependency') or meta.get('preset'):
                kind, target = 'web' if typ == 'web' else 'preset', path
            elif package and project:
                kind, target = 'scene', path
            else:
                if not target or target.suffix.lower() not in VIDEO:
                    target = next((p for p in entries(path) if p.is_file() and p.suffix.lower() in VIDEO), None)
                kind, target = ('video', target) if target else ('unsupported', path)
            if not project and not target.is_file():
                raise ValueError('No es un fondo de Workshop')
            tags = tuple(clean(t) for t in meta.get('tags', []) if isinstance(t, str)) if isinstance(meta.get('tags'), list) else ()
            preset = meta.get('preset')
            return cls(name, target, kind, 'steam', f'we:{path.name}' if path.name.isdecimal() else f'path:{key(path)}',
                       preview, package, tags, clean(meta.get('author', '')), str(meta.get('dependency') or ''),
                       preset if isinstance(preset, dict) else None,
                       project_dir=None if kind == 'video' else path)
        ext = path.suffix.lower()
        if not path.is_file() or ext not in IMAGES | VIDEO:
            raise ValueError(f'Formato no admitido: {path}')
        stem = re.sub(r'_(4k|1440p|1080p)$', '', path.stem, flags=re.I) if source == 'motion' else path.stem
        name = stem.replace('-', ' ').replace('_', ' ').title() if source == 'motion' else stem
        preview = path if ext in IMAGES else next(
            (p for s in (path.stem, stem) for e in ('.jpg', '.png', '.jpeg', '.webp')
             if (p := path.with_name(s + e)).is_file()), None)
        return cls(clean(name), path, 'image' if ext in IMAGES else 'video', source, f'file:{key(str(path).casefold())}', preview)


def steam_roots(engine):
    roots = [engine.parent.parent.parent, Path(r'C:\Program Files (x86)\Steam'),
             Path(r'C:\Program Files\Steam'), Path(r'D:\SteamLibrary'), Path(r'E:\SteamLibrary')]
    try:
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r'Software\Valve\Steam') as reg:
            roots.append(Path(winreg.QueryValueEx(reg, 'SteamPath')[0]))
    except OSError:
        pass
    for steam in tuple(roots):
        try:
            vdf = (steam / 'steamapps/libraryfolders.vdf').read_text(encoding='utf-8-sig')
            roots.extend(Path(p.replace('\\\\', '\\')) for p in re.findall(r'"path"\s+"([^"]+)"', vdf))
        except (OSError, UnicodeError):
            pass
    return dict.fromkeys(p.resolve() for p in roots)


def steam_dirs(engine):
    dirs = [p / 'steamapps/workshop/content/431960' for p in steam_roots(engine)]
    dirs.extend((ROOT / 'tools/steamcmd/steamapps/workshop/content/431960',
                 ROOT / 'tools/depotdownloader/downloads/431960'))
    return dict.fromkeys(p.resolve() for p in [*dirs, ROOT / 'media'])


def discover(engine):
    found = {}
    for root in steam_dirs(engine):
        for path in entries(root):
            if not path.is_dir():
                continue
            try:
                item = Media.from_path(path)
            except (OSError, ValueError):
                continue
            previous = found.get(item.id)
            if not previous or (item.playable, bool(item.preview)) > (previous.playable, bool(previous.preview)):
                found[item.id] = item
    for source, root in (('motion', DOWNLOADS), ('imported', DOWNLOADS / 'imported')):
        for path in entries(root):
            if path.suffix.lower() not in (VIDEO if source == 'motion' else IMAGES | VIDEO):
                continue
            try:
                item = Media.from_path(path, source)
                found[item.id] = item
            except (OSError, ValueError):
                pass
    for id, item in tuple(found.items()):
        base = found.get(f'we:{item.dependency}')
        if item.kind == 'preset' and base and base.scene and base.package:
            preset = {}
            for name, value in (item.preset or {}).items():
                if isinstance(value, str) and value.startswith(('files/', 'files\\')):
                    file = child(item.path, value)
                    if not file:
                        continue
                    value = file.as_posix()
                if isinstance(value, (bool, int, float, str)):
                    preset[name] = value
            found[id] = replace(item, path=base.path, package=base.package, kind='scene', preset=preset)
    return sorted(found.values(), key=lambda item: (item.name.casefold(), str(item.path)))


def remote(url, target, seconds=14, max_size=4_000_000, cancel=None):
    args = ['curl.exe', '-fsSL', '--compressed', '--proto', '=https', '--proto-redir', '=https',
            '--max-time', str(seconds), '--max-filesize', str(max_size), '-A', AGENT,
            '-o', str(target), '--', url]
    quiet = dict(stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, creationflags=CREATE_NO_WINDOW)
    if cancel is None:
        return subprocess.run(args, timeout=seconds + 2, **quiet).returncode == 0
    with subprocess.Popen(args, **quiet) as child:
        while child.poll() is None:
            if cancel.wait(.05):
                child.kill()
                return False
        return child.returncode == 0 and not cancel.is_set()


def online_preview(row, folder, cancel):
    if 'slug' in row:
        slug, id = row['slug'], row['id']
        if not re.fullmatch(r'[-\w]+', slug, re.ASCII) or not id.isdecimal():
            return None
        url = f'https://motionbgs.com/media/{id}/{slug}.960x540.mp4'
        file = folder / 'preview.mp4'
        if remote(url, file, seconds=8, max_size=12_000_000, cancel=cancel):
            with file.open('rb') as video:
                if video.read(12)[4:8] == b'ftyp':
                    return file
        return None
    url = row.get('image', '')
    parsed = urlparse(url)
    if parsed.scheme != 'https' or parsed.username or parsed.password or parsed.hostname not in (
            'images.steamusercontent.com', 'steamuserimages-a.akamaihd.net', 'cdn.steamusercontent.com'):
        return None
    file = folder / 'preview.gif'
    if remote(url, file, seconds=5, cancel=cancel):
        from PIL import Image
        with Image.open(file) as cover:
            if cover.format in ('GIF', 'WEBP', 'PNG') and cover.is_animated:
                return file
    return None


def steam_page(body):
    match = re.search(r'<script[^>]+id="valve-ssr-data"[^>]*>(.*?)</script>', body, re.S)
    if match:
        context = json.loads(match[1])['renderContext']
        for query in json.loads(context['queryData'])['queries']:
            data = query['state'].get('data')
            if isinstance(data, dict):
                data = data.get('data', data)
                if isinstance(data, dict) and 'current_page' in data and 'results' in data:
                    rows = []
                    for item in (data.get('results') or [])[:30]:
                        id = str(item['publishedfileid'])
                        if not id.isdecimal():
                            continue
                        tags = [str(tag.get('tag', '')) for tag in item.get('tags', [])]
                        size = str(item.get('file_size', ''))
                        rows.append(dict(id=id, name=clean(item['title']), image=item.get('preview_url', ''),
                                         tags=tags, size_bytes=int(size) if size.isdecimal() else None,
                                         url=f'https://steamcommunity.com/sharedfiles/filedetails/?id={id}'))
                    return rows, data['current_page'] < data.get('total_pages', 0), ''
    # Steam can still serve the old markup.
    cards = re.findall(r'<a\s+href="https://steamcommunity\.com/sharedfiles/filedetails/\?id=(\d+)"[^>]*>\s*<img\s+src="([^"]+)"\s+alt="([^"]+)"', body)
    if cards:
        return [dict(id=i, image=unescape(pic), name=clean(t), tags=[],
                     url=f'https://steamcommunity.com/sharedfiles/filedetails/?id={i}') for i, pic, t in cards[:30]], 'pagingNext' in body, ''
    raise ValueError('Steam no entregó resultados')


def motion_page(body):
    cards = re.findall(r'<a\s+title="[^"]+"\s+href=["\']?(/[-\w]+)["\']?\s*>(.*?)</a>', body, re.S | re.I)
    rows = []
    for link, card in cards[:120]:
        pic = re.search(r'/i/c/364x205/media/(\d+)/([-\w]+)(?:\.(\d+x\d+))?\.jpg', card)
        title = re.search(r'<span class=["\']?ttl["\']?>(.*?)</span>', card, re.S)
        if not pic or not title:
            continue
        id, slug, res = pic.groups()
        hd = pic[0].replace('/364x205/', '/546x308/')
        rows.append(dict(id=id, name=clean(title[1]), image='https://motionbgs.com' + (hd if hd in card else pic[0]),
                         slug=slug, url=f'https://motionbgs.com{link}',
                         resolution='4K' if res == '3840x2160' or re.search(r'class=frm>\s*4K\b', card) else '1080p'))
    next_tag = re.search(r'<link href=["\']?https://motionbgs\.com/tag:([-\w]+)/\d+/["\']?\s+rel=["\']?next', body)
    if not rows and 'No Results Found' not in body and 'No wallpapers found' not in body:
        raise ValueError('MotionBGs no entregó resultados')
    return rows, bool(rows and next_tag), next_tag[1] if next_tag else ''


def online(req):
    q = req['q'].strip()
    page = max(1, min(1000, int(req['page'])))
    tag = int(req['tag'])
    if req['source'] == 'steam':
        args = dict(appid=431960, section='readytouseitems',
                    browsesort=SORTS[int(req['sort'])], p=page, numperpage=30, l='english')
        if q:
            args['searchtext'] = q
        if STEAM[tag]:
            args['requiredtags[]'] = STEAM[tag]
        url = 'https://steamcommunity.com/workshop/browse/?' + urlencode(args)
        parse, ttl = steam_page, 300
    else:
        cat = req.get('next_tag', '') if page > 1 and q else MOTION[tag]
        if not re.fullmatch(r'[-a-z0-9_]+', cat):
            cat = MOTION[tag]
        url = ('https://motionbgs.com/search?' + urlencode({'q': q}) if page == 1 and q else
               f'https://motionbgs.com/tag:{cat}/' + (f'{page}/' if page > 1 else ''))
        parse, ttl = motion_page, 43200
    cache = CACHE / 'pages'
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / (key(url) + '.html')
    if path.is_file() and time.time() - path.stat().st_mtime < ttl:
        return parse(path.read_text(encoding='utf-8'))
    with tempfile.TemporaryDirectory() as temp:
        scratch = Path(temp) / 'page'
        if remote(url, scratch):
            body = scratch.read_text(encoding='utf-8', errors='replace')
            try:
                result = parse(body)
                atomic(path, body.encode('utf-8'))
                prune(cache, 64)
                return result
            except (ValueError, KeyError):
                if not path.is_file():
                    raise
        if path.is_file():
            return parse(path.read_text(encoding='utf-8'))
        raise ValueError('No se pudo conectar con el catálogo')


def prune(folder, limit):
    files = [p for p in entries(folder) if p.is_file() and p.suffix != '.part']
    for p in sorted(files, key=lambda p: p.stat().st_mtime, reverse=True)[limit:]:
        p.unlink(missing_ok=True)


def thumbs(paths):
    from concurrent.futures import ThreadPoolExecutor
    from io import BytesIO
    from threading import Lock
    from PIL import Image, ImageDraw, ImageFont, ImageOps

    target = CACHE / POSTERS
    target.mkdir(parents=True, exist_ok=True)
    frame_lock = Lock()
    width, height = POSTER_SIZE

    def video_frame(path, frame):
        with frame_lock:
            decoder = shutil.which('ffmpeg')
            if not decoder or subprocess.run(
                [decoder, '-nostdin', '-loglevel', 'error', '-protocol_whitelist', 'file',
                 '-threads', '1', '-ss', '0.3', '-i', str(path), '-frames:v', '1',
                 '-filter_threads', '1', '-vf',
                 f'scale={width}:{height}:force_original_aspect_ratio=increase:flags=lanczos,'
                 f'crop={width}:{height},setsar=1', '-threads', '1', str(frame)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                creationflags=CREATE_NO_WINDOW | 0x40, timeout=8,
            ).returncode != 0:
                raise OSError('Frame unavailable')
        return frame

    def image_from(path):
        with Image.open(path) as image:
            if image.width * image.height > 32_000_000:
                raise ValueError('Poster too large')
            return ImageOps.exif_transpose(image).convert('RGB')

    def one(value):
        id, source, name, video = value
        dst = target / (id + '.png')
        if dst.is_file():
            return
        try:
            canvas = Image.new('RGB', (width, height + 40), (0, 0, 0))
            canvas.paste((18, 14, 37), (0, 0, width, height))
            try:
                if source:
                    with tempfile.TemporaryDirectory() as tmp:
                        path = Path(source)
                        if source.startswith('https://'):
                            parsed = urlparse(source)
                            if parsed.hostname not in ('motionbgs.com', 'images.steamusercontent.com',
                                                                'steamuserimages-a.akamaihd.net', 'cdn.steamusercontent.com'):
                                raise ValueError('Unknown poster host')
                            path = Path(tmp) / 'thumb'
                            url = source
                            if parsed.hostname != 'motionbgs.com':
                                params = [(k, v) for k, v in parse_qsl(parsed.query, keep_blank_values=True)
                                          if k not in ('imw', 'imh')]
                                url = parsed._replace(query=urlencode(params + [('imw', 640)])).geturl()
                            if not (remote(url, path, seconds=6, max_size=6_000_000)
                                    or url != source and remote(source, path, seconds=4, max_size=6_000_000)):
                                raise OSError('Poster unavailable')
                        elif path.suffix.lower() in VIDEO:
                            path = video_frame(path, Path(tmp) / 'frame.png')
                        image = image_from(path)
                        if video and (image.width < width or image.height < height):
                            try:
                                image = image_from(video_frame(Path(video), Path(tmp) / 'frame.png'))
                            except (OSError, ValueError, subprocess.SubprocessError):
                                pass  # Keep the available cover if decoding fails.
                        canvas.paste(ImageOps.fit(image, POSTER_SIZE, Image.Resampling.LANCZOS))
            except (OSError, ValueError, subprocess.SubprocessError):
                if not unicode_title(name):
                    return
            if unicode_title(name):
                font_root = Path(os.environ.get('WINDIR', r'C:\Windows')) / 'Fonts'
                if any('\u3040' <= ch <= '\u30ff' for ch in name):
                    face = 'YuGothB.ttc'
                elif any('\uac00' <= ch <= '\ud7a3' for ch in name):
                    face = 'malgunbd.ttf'
                elif any('\u3400' <= ch <= '\u9fff' for ch in name):
                    face = 'msyhbd.ttc'
                else:
                    face = 'seguisb.ttf'
                font = ImageFont.truetype(font_root / face, 26)
                limit = width - 40
                if font.getlength(name) <= limit:
                    ImageDraw.Draw(canvas).text((width // 2, height + 20), name, fill='white', font=font, anchor='mm')
                else:
                    cut = next(i for i in range(1, len(name) + 1) if font.getlength(name[:i]) > limit) - 1
                    # Wrap on a word boundary when there is one near the edge.
                    space = name.rfind(' ', cut // 2, cut + 1)
                    if space > 0:
                        cut = space
                    first, second = name[:cut].rstrip(), name[cut:].lstrip()
                    if font.getlength(second) > limit:
                        while font.getlength(second.rstrip() + '…') > limit:
                            second = second[:-1]
                        second = second.rstrip() + '…'
                    # Two squashed lines plus a marker in the unused left margin.
                    line = Image.new('L', (width, 32))
                    draw = ImageDraw.Draw(line)
                    for i, label in enumerate((first, second)):
                        draw.rectangle((0, 0, width - 1, 31), fill=0)
                        draw.text((width // 2, 16), label, fill=255, font=font, anchor='mm')
                        canvas.paste(line.resize((width, 20), Image.Resampling.LANCZOS), (0, height + 20 * i))
                    ImageDraw.Draw(canvas).rectangle((0, height, 7, height + 7), fill='white')
            encoded = BytesIO()
            canvas.convert('RGBA').save(encoded, format='PNG', compress_level=3)
            atomic(dst, encoded.getvalue())
        except (OSError, ValueError, subprocess.SubprocessError):
            pass

    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(one, paths))
    prune(target, 256)


def import_file():
    # Open the Windows picker in an on-demand helper, outside the renderer.
    script = ('Add-Type -AssemblyName System.Windows.Forms; '
              '$d=New-Object System.Windows.Forms.OpenFileDialog; '
              '$d.Title="Agregar fondo"; $d.Filter="Fondos|*.mp4;*.webm;*.m4v;*.mov;*.wmv;*.mkv;*.avi;*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp;*.avif;*.tif;*.tiff"; '
              'if ($d.ShowDialog() -eq "OK") { [Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.FileName) }')
    path = subprocess.check_output(['powershell.exe', '-NoProfile', '-STA', '-Command', script],
                                   creationflags=CREATE_NO_WINDOW).decode('utf-8').strip()
    if not path:
        return None
    item = Media.from_path(Path(path))
    dst = DOWNLOADS / 'imported' / item.path.name
    dst.parent.mkdir(parents=True, exist_ok=True)
    if dst.resolve() != item.path:
        # A different file with the same name must never be overwritten.
        for n in range(1, 10000):
            try:
                with dst.open('xb') as out, item.path.open('rb') as src:
                    try:
                        shutil.copyfileobj(src, out)
                    except BaseException:
                        out.close()
                        dst.unlink(missing_ok=True)
                        raise
                break
            except FileExistsError:
                dst = dst.with_name(f'{item.path.stem} ({n}){item.path.suffix}')
        else:
            raise ValueError('No se pudo elegir un nombre libre')
    return str(dst)


def download(row):
    slug, id = row['slug'], row['id']
    if not re.fullmatch(r'[-a-zA-Z0-9_]+', slug) or not id.isdecimal():
        raise ValueError('Fondo inválido')
    DOWNLOADS.mkdir(parents=True, exist_ok=True)
    sizes = [('3840x2160', '4k'), ('2560x1440', '1440p'), ('1920x1080', '1080p')]
    if row['resolution'] != '4K':
        sizes = sizes[1:]
    for res, label in sizes:
        dst = DOWNLOADS / f'{slug}_{label}.mp4'
        if dst.is_file():
            return str(dst)
        with tempfile.TemporaryDirectory(prefix='ika-download-', dir=DOWNLOADS) as tmp:
            part = Path(tmp) / 'wallpaper.mp4'
            url = f'https://motionbgs.com/media/{id}/{slug}.{res}.mp4'
            if remote(url, part, seconds=90, max_size=1_500_000_000):
                with part.open('rb') as video:
                    if video.read(12)[4:8] != b'ftyp':
                        continue
                try:
                    part.rename(dst)
                except FileExistsError:
                    pass
                preview = DOWNLOADS / (slug + '.jpg')
                if not preview.is_file():
                    try:
                        poster = Path(tmp) / 'poster.jpg'
                        if remote(row['image'], poster, seconds=5):
                            poster.rename(preview)
                    except (OSError, subprocess.SubprocessError):
                        pass
                return str(dst)
    raise ValueError('No se pudo descargar el video')


def import_url(url):
    parsed = urlparse(url)
    name = Path(unquote(parsed.path)).name
    ext = Path(name).suffix.lower()
    if (parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password
            or ext not in IMAGES | VIDEO):
        raise ValueError('Usa un enlace HTTPS directo a un video o imagen')
    folder = DOWNLOADS / 'imported'
    folder.mkdir(parents=True, exist_ok=True)
    stem = re.sub(r'[^\w.-]+', '-', Path(name).stem, flags=re.UNICODE).strip('.-')[:80] or 'Fondo'
    dst = folder / f'{stem}-{key(url)[:8]}{ext}'
    if dst.is_file():
        return str(dst)
    with tempfile.TemporaryDirectory(prefix='ika-download-', dir=folder) as tmp:
        part = Path(tmp) / dst.name
        if not remote(url, part, seconds=90, max_size=1_500_000_000) or not part.stat().st_size:
            raise ValueError('No se pudo descargar el archivo')
        try:
            part.rename(dst)
        except FileExistsError:
            pass
    return str(dst)


def main():
    job = Path(sys.argv[1])
    action, data, result = json.loads(job.read_text(encoding='utf-8'))
    try:
        if action == 'online':
            rows, more, tag = online(data)
            value = dict(rows=rows, more=more, next_tag=tag)
        elif action == 'thumbs':
            thumbs(data)
            return
        elif action == 'download':
            value = dict(path=download(data))
        elif action == 'import':
            value = dict(path=import_file())
        elif action == 'url':
            value = dict(path=import_url(data))
        else:
            raise ValueError('Trabajo de catálogo desconocido')
    except (OSError, ValueError, KeyError, IndexError, subprocess.SubprocessError) as error:
        value = dict(error=str(error))
    publish(Path(result), value)


if __name__ == '__main__':
    main()
