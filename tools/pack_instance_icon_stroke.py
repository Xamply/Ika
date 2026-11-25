"""Build the WebView2 instance icon outline ramp and its soft backing."""

from bisect import bisect_right
from pathlib import Path
import os
import re
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter

root = Path(__file__).resolve().parent.parent
legacy = Path(sys.argv[1] if len(sys.argv) > 1 else os.environ.get("IKA_LEGACY_DIR", ""))
if not (legacy / "index.html").is_file():
    sys.exit("Pass a checkout of the legacy-webview2 branch (or set IKA_LEGACY_DIR).")
html = (legacy / "index.html").read_text(encoding="utf-8")
gradient = html.split('id="coleccionGodStrokeGrad"', 1)[1].split("</linearGradient>", 1)[0]
positions, colors = zip(*[
    (float(percent) / 100, bytes.fromhex(color))
    for percent, color in re.findall(r'offset="([\d.]+)%" stop-color="#([\da-fA-F]{6})"', gradient)
])
assert positions[0] == 0 and positions[-1] == 1

ramp = bytearray()
for index in range(256):
    t = index / 255
    i = min(bisect_right(positions, t), len(positions) - 1)
    alpha = (t - positions[i - 1]) / (positions[i] - positions[i - 1])
    ramp.extend(round(a + (b - a) * alpha) for a, b in zip(colors[i - 1], colors[i]))
(root / "src/profile/instance-icon-stroke.rgb").write_bytes(ramp)

# Render the CSS box shadow and the pink drop shadow of the SVG hairline
# together. Keep the outline itself separate so its gradient can still rotate.
scale, pad, side = 4, 16, 50
size = (side + 2 * pad) * scale

def rounded(inset, radius):
    mask = Image.new("L", (size, size))
    first = (pad + inset) * scale
    end = size - 1 - first
    ImageDraw.Draw(mask).rounded_rectangle((first, first, end, end), radius * scale, fill=255)
    return mask

def tint(mask, rgb, alpha=1):
    layer = Image.new("RGBA", mask.size, rgb)
    layer.putalpha(mask.point(lambda a: round(a * alpha)))
    return layer

base = rounded(0, 10)
plate = tint(base.filter(ImageFilter.GaussianBlur(8 * scale)), (139, 92, 246), 0.16)
plate.alpha_composite(tint(base, (20, 15, 49)))
stroke = ImageChops.subtract(rounded(-0.4, 10.9), rounded(1.4, 9.1))
plate.alpha_composite(tint(stroke.filter(ImageFilter.GaussianBlur(4 * scale)), (255, 78, 210), 0.8))
plate.resize((size // 2, size // 2), Image.Resampling.LANCZOS).save(
    root / "src/profile/instance-icon-plate.png", optimize=True,
)
