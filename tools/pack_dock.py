"""Pack the legacy WebView2 dock paint, masks and ramps for the native renderer."""

from bisect import bisect_right
import json
from pathlib import Path
import re
import sys

from PIL import Image, ImageFilter, ImageChops
import pack_wallpapers

root = Path(__file__).resolve().parent.parent
scratch = root / "scratch/dock-reference"
out = root / "src/dock"
out.mkdir(exist_ok=True)
layout = json.loads((scratch / "layout.json").read_text(encoding="utf-8"))
layers = layout["layers"]
tiles = {}


def load(key):
    return Image.open(scratch / f"{key}.png").convert("RGBA")


def add(key, image, origin, *, premultiply=True, trim=True):
    box = image.getchannel("A").getbbox() if trim else (0, 0, *image.size)
    assert box, f"Empty dock layer: {key}"
    x, y, _, _ = box
    image = image.crop(box)
    # Interpolating premultiplied paint avoids dark fringes around icons.
    if premultiply:
        image = Image.merge("RGBA", image.convert("RGBa").split())
    tiles[key] = image, (origin[0] + x, origin[1] + y)


def play_states():
    import numpy as np

    layout = json.loads((scratch / "play-state-layout.json").read_text(encoding="utf-8"))
    for key, box in layout['layers'].items():
        add(key, load(key), box[:2])
    # Sample the original SVG centerline. RGB stores the unit phase vector
    # and signed distance in dock pixels for the laser, shockwave, and flash.
    points = np.array(layout['pathPoints'])
    ends = np.roll(points, -1, axis=0)
    left, top, width, height = 1091, 153, 378, 96
    x = (np.arange(width) + left + .5)[:, None]
    dx2 = (x - points[:, 0]) ** 2
    phase = np.arange(len(points)) * (2 * np.pi / len(points))
    image = np.full((height, width, 4), 255, dtype=np.uint8)
    for row in range(height):
        y = row + top + .5
        d2 = dx2 + (points[:, 1] - y) ** 2
        nearest = d2.argmin(axis=1)
        distance = np.sqrt(d2[np.arange(width), nearest])
        crossings = (points[:, 1] > y) != (ends[:, 1] > y)
        a, b = points[crossings], ends[crossings]
        if len(a):
            edges = a[:, 0] + (b[:, 0] - a[:, 0]) * (y - a[:, 1]) / (b[:, 1] - a[:, 1])
            distance[(x[:, 0] > edges.min()) & (x[:, 0] < edges.max())] *= -1
        image[row, :, 0] = np.round((np.cos(phase[nearest]) + 1) * 127.5)
        image[row, :, 1] = np.round((np.sin(phase[nearest]) + 1) * 127.5)
        image[row, :, 2] = np.round(np.clip(distance / 16 + .5, 0, 1) * 255)
        image[row, :, 3] = (np.abs(distance) <= 2.0) * 255
    add('play_outline', Image.fromarray(image), (left, top), premultiply=False, trim=False)


play_states()
play_keys = tuple(tiles)
if "--play-states-only" in sys.argv:
    # Append the new artwork without moving any existing dock/catalog tiles.
    atlas = Image.open(out / "atlas.png").convert("RGBA")
    wgsl = (out / "atlas.wgsl").read_text(encoding="utf-8")
    marker = re.search(r"^const DOCK_PLAY_TEXT_PRESS = DockSprite\(vec4\(1.0, (\d+).0", wgsl, re.M)
    if marker and re.search(r"^// Play states appended to the atlas\.$", wgsl, re.M):
        atlas = atlas.crop((0, 0, atlas.width, int(marker[1]) - 1))
    wgsl = re.sub(r"^// Play states appended to the atlas\.\n", "", wgsl, flags=re.M)
    for name in play_keys:
        wgsl = re.sub(rf"^const DOCK_{name.upper()} = .*;\n", "", wgsl, flags=re.M)
    rebuilt = Image.new("RGBA", (atlas.width, atlas.height + max(im.height for im, _ in tiles.values()) + 2))
    rebuilt.paste(atlas, (0, 0))
    x, y = 1, atlas.height + 1
    consts = "// Play states appended to the atlas.\n"
    for name, (im, (ox, oy)) in tiles.items():
        assert x + im.width < atlas.width
        rebuilt.paste(im, (x, y))
        consts += f"const DOCK_{name.upper()} = DockSprite(vec4({x}.0, {y}.0, {im.width}.0, {im.height}.0), vec2({ox}.0, {oy}.0));\n"
        x += im.width + 2
    rebuilt.save(out / "atlas.png", optimize=True)
    (out / "atlas.wgsl").write_text(wgsl + consts, encoding="utf-8")
    raise SystemExit


if "--active-tray-only" in sys.argv:
    # Update just this tile and keep all existing atlas placements identical.
    rect = json.loads((scratch / "left-active-layout.json").read_text(encoding="utf-8"))
    add("left_active", load("left_active"), rect[:2])
    tile, (x, y) = tiles["left_active"]
    atlas = Image.open(out / "atlas.png").convert("RGBA")
    wgsl = (out / "atlas.wgsl").read_text(encoding="utf-8")
    marker = re.search(r"^const DOCK_LEFT_ACTIVE = DockSprite\(vec4\(([\d.]+), ([\d.]+)", wgsl, re.M)
    if marker and tuple(map(lambda n: int(float(n)), marker.groups())) == (1, atlas.height-tile.height-1):
        atlas = atlas.crop((0, 0, atlas.width, atlas.height-tile.height-2))
    rebuilt = Image.new("RGBA", (atlas.width, atlas.height + tile.height + 2))
    rebuilt.paste(atlas, (0, 0))
    rebuilt.paste(tile, (1, atlas.height + 1))
    rebuilt.save(out / "atlas.png", optimize=True)
    wgsl = re.sub(r"^const DOCK_LEFT_ACTIVE = .*;\n", "", wgsl, flags=re.M)
    index = wgsl.index("const DOCK_RIGHT")
    sprite = f"const DOCK_LEFT_ACTIVE = DockSprite(vec4(1.0, {atlas.height+1}.0, {tile.width}.0, {tile.height}.0), vec2({x}.0, {y}.0));\n"
    (out / "atlas.wgsl").write_text(wgsl[:index] + sprite + wgsl[index:], encoding="utf-8")
    raise SystemExit

for key, (x, y, _, _) in layers.items():
    if key.startswith("edge"):
        continue
    if key.startswith("tip"):
        ax, ay = layout["tipAnchors"][int(key[3:])]
        x, y = x - ax, y - ay
    im = load(key)
    if key == 'shadow':
        # This is the shadow of the complete outline. It is smooth enough to
        # store one mirrored half at half resolution, with a flat continuation.
        alpha = im.getchannel('A').resize((im.width // 2, im.height // 2), Image.Resampling.BOX)
        im = Image.new('RGBA', alpha.size)
        im.putalpha(alpha)
        add(key, im, (x, y), trim=False)
    elif key == 'play_icon':
        alpha = im.getchannel('A')
        near = alpha.filter(ImageFilter.GaussianBlur(1))
        far = alpha.filter(ImageFilter.GaussianBlur(6.3))
        bounds = ImageChops.lighter(alpha, far).getbbox()
        im = Image.merge('RGBA', (near, far, Image.new('L', im.size), alpha)).crop(bounds)
        add(key, im, (x + bounds[0], y + bounds[1]), trim=False, premultiply=False)
    elif key in ('play_track', 'play_hud'):
        # Store a unit vector for the authored path distance. The angle stays
        # continuous across the join when the GPU interpolates the texture.
        from math import cos, sin, tau
        points = layout['track']
        for py in range(im.height):
            for px in range(im.width):
                alpha = im.getpixel((px, py))[3]
                if not alpha:
                    continue
                cx, cy = x + px + .5, y + py + .5
                i = min(range(len(points)), key=lambda i: (points[i][0]-cx)**2 + (points[i][1]-cy)**2)
                phase = tau * i / len(points)
                im.putpixel((px,py), (round((cos(phase)+1)*127.5), round((sin(phase)+1)*127.5), 0, alpha))
        add(key, im, (x,y))
    else:
        add(key, im, (x, y))

edges = [load(f"edge{i}").getchannel("A") for i in range(4)]
assert all(mask.getbbox() for mask in edges)
add("edges", Image.merge("RGBA", edges), layers["edge0"][:2], premultiply=False, trim=False)

# Rasterize the authored SVG gradient stops in sRGB. Gradients share one
# narrow strip in the same texture as the dock; five/six sprite reads are not
# required to reconstruct each gradient stop at runtime.
gradients = json.loads((scratch / "gradients.json").read_text(encoding="utf-8"))
ramps = Image.new("RGBA", (512, len(gradients)))
for y, stops in enumerate(gradients):
    positions, colors = zip(*[(t, bytes.fromhex(c)) for t, c in stops])
    for x in range(ramps.width):
        t = x / (ramps.width - 1)
        j = min(bisect_right(positions, t), len(stops) - 1)
        factor = min(1, (t - positions[j-1]) / (positions[j] - positions[j-1]))
        rgb = tuple(round(a + (b-a)*factor) for a, b in zip(colors[j-1], colors[j]))
        ramps.putpixel((x, y), (*rgb, 255))
add("ramps", ramps, (0, 0), premultiply=False)
# The native Preferences overlay samples the same rightward gradient as Wallpapers.
(out / "wallpaper-stroke.rgb").write_bytes(ramps.crop((0, 1, 512, 2)).convert("RGB").tobytes())
pack_wallpapers.populate(add)

# Pack tallest first into shelves, leaving one transparent pixel on every
# side of each rect so neighboring artwork never bleeds through the sampler.
width = 1024
rows, placements = [], {}
for key, (im, _) in sorted(tiles.items(), key=lambda item: -item[1][0].height):
    w, h = im.size
    assert w + 2 <= width
    for row in rows:
        if row[1] >= h + 2 and row[2] + w + 2 <= width:
            break
    else:
        row = [sum(r[1] for r in rows), h + 2, 0]
        rows.append(row)
    placements[key] = (row[2] + 1, row[0] + 1, w, h)
    row[2] += w + 2

atlas = Image.new("RGBA", (width, sum(r[1] for r in rows)))
for key, (im, _) in tiles.items():
    atlas.paste(im, placements[key][:2])
atlas.save(out / "atlas.png", optimize=True)


def num(n):
    return str(float(round(n, 6)))


def vec(values):
    return f"vec{len(values)}(" + ", ".join(map(num, values)) + ")"


def sprite(key):
    if key not in tiles:
        return "DockSprite(vec4(0.0), vec2(0.0))"
    return f"DockSprite({vec(placements[key])}, {vec(tiles[key][1])})"


wgsl = "// Generated by tools/pack_dock.py. Paint is premultiplied sRGB at 1x.\n"
wgsl += "struct DockSprite { rect: vec4<f32>, origin: vec2<f32> }\n"
for name in ("chassis", "shadow", "play", "play_hot", "play_edge", "play_sheen", "play_text", "play_text_hot", "play_icon", "play_track", "play_hud", "left", "left_active", "right", "wallpaper_label", "wallpaper_mask", "edges", "ramps"):
    wgsl += f"const DOCK_{name.upper()} = {sprite(name)};\n"
for name in play_keys:
    wgsl += f"const DOCK_{name.upper()} = {sprite(name)};\n"
for name in pack_wallpapers.LABELS:
    wgsl += f"const WP_{name.upper()} = {sprite('wp_' + name)};\n"
wgsl += f"const WP_GLYPHS = {sprite('wp_glyphs')};\n"
wgsl += 'const WP_POSTERS = array(' + ', '.join(sprite(f'wp_poster{i}') for i in range(1, 4)) + ');\n'
for key in ("icon", "hot", "tip"):
    values = ",\n    ".join(sprite(f"{key}{i}") for i in range(13))
    wgsl += f"const DOCK_{key.upper()}S = array(\n    {values},\n);\n"
boxes = [rect for rect, _ in layout["buttons"]]
wgsl += "const DOCK_BUTTONS = array(\n    " + ",\n    ".join(map(vec, boxes)) + ",\n);\n"
(out / "atlas.wgsl").write_text(wgsl, encoding="utf-8")
rust = "// Generated by tools/pack_dock.py from the reference's DOM layout.\n"
rust += "pub const BUTTONS: [[f32; 4]; 13] = [\n"
for box in boxes:
    rust += "    [" + ", ".join(map(num, box)) + "],\n"
rust += "];\n"
(out / "layout.rs").write_text(rust, encoding="utf-8")
print(f"Dock atlas: {atlas.width} x {atlas.height}, {(out / 'atlas.png').stat().st_size:,} bytes on disk")
