"""Package the WebView2 launcher's original instance picker artwork."""
from pathlib import Path
import re
import sys
from zipfile import ZipFile, ZipInfo

from PIL import Image


def main():
    original = Path(sys.argv[1])
    output = Path(__file__).resolve().parents[1] / "src/home/instance-art"
    output.mkdir(parents=True, exist_ok=True)
    js = (original / "src/main.js").read_text(encoding="utf-8")
    pool = js.split("const COZY_COVERS_POOL = [", 1)[1].split("];", 1)[0]
    covers = re.findall(r'"/?\./(instance_covers/thumbs/[^"]+\.jpg)"', pool)
    assert len(covers) == 40 and len(set(covers)) == len(covers)
    with ZipFile(output / "covers.zip", "w") as archive:
        for cover in covers:
            archive.writestr(ZipInfo(Path(cover).name), (original / "public" / cover).read_bytes())
    (output / "covers.txt").write_text(
        "\n".join(Path(cover).stem for cover in covers) + "\n", encoding="utf-8"
    )

    # WebView2 prefers the launcher directory's curated block textures.
    blocks = sorted((original / "minecraft_block_textures").glob("*.png"))
    if not blocks:
        blocks = sorted((original / "public/minecraft_block_textures").glob("*.png"))
    assert blocks
    columns, cell = 24, 32
    atlas = Image.new("RGBA", (columns * cell, ((len(blocks) + columns - 1) // columns) * cell))
    for i, path in enumerate(blocks):
        with Image.open(path) as source:
            tile = source.convert("RGBA").resize((cell, cell), Image.Resampling.NEAREST)
        atlas.paste(tile, (i % columns * cell, i // columns * cell))
    atlas.save(output / "blocks.png", optimize=True)
    (output / "blocks.txt").write_text("\n".join(block.stem for block in blocks) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
