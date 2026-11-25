"""Pack the original WebView2 carousel covers and Minecraft block icons into wgpu assets."""
import argparse
import re
from pathlib import Path

from PIL import Image, ImageOps

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("webview_project", type=Path)
root = parser.parse_args().webview_project
js = (root / "src/main.js").read_text(encoding="utf-8")
public = root / "public"
out = Path(__file__).resolve().parents[1] / "src/home"
out.mkdir(exist_ok=True)
covers = re.search(r"const COZY_COVERS_POOL = \[(.*?)\];", js, re.S)[1]
covers = re.findall(r'"\./([^"]+\.jpg)"', covers)
assert len(covers) == 40
atlas = Image.new("RGB", (320 * 5, 192 * 8))
for i, file in enumerate(covers):
    with Image.open(public / file) as image:
        atlas.paste(ImageOps.fit(image.convert("RGB"), (320, 192)), (i % 5 * 320, i // 5 * 192))
atlas.save(out / "covers.jpg", quality=92, subsampling=0, optimize=True)
blocks = re.search(r"const MINECRAFT_BLOCK_SIDE_TEXTURES = \[(.*?)\];", js, re.S)[1]
blocks = re.findall(r'"/([^"]+\.png)"', blocks)
blocks = [file for file in blocks if (public / file).is_file()][:24]
assert len(blocks) == 24
icons = Image.new("RGBA", (48 * 24, 48))
for i, file in enumerate(blocks):
    with Image.open(public / file) as image:
        icons.paste(ImageOps.fit(image.convert("RGBA"), (48, 48), Image.Resampling.NEAREST), (i * 48, 0))
icons.save(out / "blocks.png", optimize=True)

# Preserve the exact 293-stop WebView2 border gradient in a small LUT. The
# native shader rotates the same user-space gradient around the card center.
html = (root / "index.html").read_text(encoding="utf-8")
gradient = re.search(r'<linearGradient id="cardGodStrokeGrad".*?</linearGradient>', html, re.S)[0]
stops = []
for tag in re.findall(r"<stop[^>]*>", gradient):
    match = re.search(r'offset="([0-9.]+)%"[^>]*stop-color="#([0-9a-fA-F]{6})"', tag)
    if match:
        stops.append((float(match[1]) / 100.0, tuple(int(match[2][i:i + 2], 16) for i in (0, 2, 4))))
assert len(stops) == 293
lut = Image.new("RGBA", (1024, 1))
pixels = []
for x in range(1024):
    t = x / 1023
    right = next((i for i, (offset, _) in enumerate(stops) if offset >= t), len(stops) - 1)
    left = max(0, right - 1)
    o0, c0 = stops[left]
    o1, c1 = stops[right]
    amount = 0.0 if o1 == o0 else max(0.0, min(1.0, (t - o0) / (o1 - o0)))
    pixels.append(tuple(round(c0[k] + (c1[k] - c0[k]) * amount) for k in range(3)) + (255,))
lut.putdata(pixels)
lut.save(out / "gradient.png", optimize=True)
