#!/usr/bin/env python3
"""Generates the workbench's two built-in wallpapers into ui/public/assets:

  stage-light.jpg  the light mode's ground: a linen cloth in a pale kaya colour
  stage-dark.jpg   the dark mode's ground: the same cloth in a walnut colour

Both are 512 x 512 tiles that repeat without a seam. The weave comes from one photograph, Poly
Haven's `rough_linen` diffuse map (CC0, by colormass and Rico Cilliers), which itself tiles: its
lighting is flattened and only its fine detail is kept, as lighter and darker shades of one colour.
The same source and Pillow build reproduce the committed files. Needs Pillow.

    python3 tools/stage_wallpapers.py [rough_linen_diff_1k.jpg]

Without a path the source is downloaded from Poly Haven.
"""
import io
import sys
import urllib.request
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter

SOURCE_URL = "https://dl.polyhaven.org/file/ph-assets/Textures/jpg/1k/rough_linen/rough_linen_diff_1k.jpg"
WORK = 1024  # the source's size, where the detail is separated
SIZE = 512  # the tile's size
OUT = Path(__file__).resolve().parent.parent / "ui" / "public" / "assets"
# Each tile's colour and how much of the weave's contrast it keeps.
TILES = {"stage-light.jpg": ((201, 186, 158), 0.8), "stage-dark.jpg": ((75, 63, 47), 0.5)}


def tiled(image: Image.Image) -> Image.Image:
    """`image` repeated 3 x 3, so a filter or a resize sees the neighbouring tiles at its edges."""
    width, height = image.size
    sheet = Image.new(image.mode, (width * 3, height * 3))
    for ty in range(3):
        for tx in range(3):
            sheet.paste(image, (tx * width, ty * height))
    return sheet


def tile(weave: Image.Image, colour: tuple[int, int, int], contrast: float) -> Image.Image:
    """`colour` shifted per pixel by the weave's fine detail: the grey source less everything wider
    than a twelfth of it, times `contrast`."""
    box = (WORK, WORK, WORK * 2, WORK * 2)
    broad = tiled(weave).filter(ImageFilter.GaussianBlur(WORK / 12)).crop(box)
    detail = ImageChops.subtract(weave, broad, scale=1 / contrast, offset=128)
    full = Image.merge("RGB", [detail.point(lambda v, c=c: max(0, min(255, v - 128 + c))) for c in colour])
    return tiled(full).resize((SIZE * 3, SIZE * 3), Image.LANCZOS).crop((SIZE, SIZE, SIZE * 2, SIZE * 2))


if __name__ == "__main__":
    source = Path(sys.argv[1]).read_bytes() if len(sys.argv) > 1 else urllib.request.urlopen(SOURCE_URL).read()
    weave = Image.open(io.BytesIO(source)).convert("L").resize((WORK, WORK), Image.LANCZOS)
    for name, (colour, contrast) in TILES.items():
        tile(weave, colour, contrast).save(OUT / name, quality=90, optimize=True)
        print(name, (OUT / name).stat().st_size, "bytes")
