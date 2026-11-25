"""Pack premultiplied Edge samples of the CRT light into a small RGBA atlas."""
from pathlib import Path

from PIL import Image, ImageChops

root = Path(__file__).resolve().parents[1]
reference = root / "scratch/crt-reference"
atlas = Image.new("RGBA", (256, 384))
for kind in ("beam", "dot"):
    for off, suffix in enumerate(("", "-off")):
        image = Image.open(reference / f"{kind}{suffix}.png").convert("RGBA")
        size = (256, 128) if kind == "beam" else (128, 128)
        # Pillow filters RGBA with premultiplication. Keep the result
        # premultiplied in the texture for bilinear sampling and compositing.
        red, green, blue, alpha = image.resize(size, Image.Resampling.LANCZOS).split()
        channels = tuple(ImageChops.multiply(color, alpha) for color in (red, green, blue))
        atlas.paste(Image.merge("RGBA", (*channels, alpha)),
                    (0, off * 128) if kind == "beam" else (off * 128, 256))
atlas.save(root / "src/crt-atlas.png", optimize=True)
