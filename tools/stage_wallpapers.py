#!/usr/bin/env python3
"""Generates the workbench's two built-in wallpapers into ui/public/assets:

  stage-ink.jpg    the dark mode's ground: dark slate, the stone black Go stones are cut from
  stage-washi.jpg  the light mode's ground: washi, the paper game records are written on

Both are 512 x 512 tiles that repeat without a seam. The output is deterministic (fixed seeds), so
running the script again reproduces the committed files. Needs Pillow.

    python3 tools/stage_wallpapers.py
"""
import math
import random
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

SIZE = 512
OUT = Path(__file__).resolve().parent.parent / "ui" / "public" / "assets"


def field(rng: random.Random, cols: int, rows: int, blur: float = 0.0) -> Image.Image:
    """A SIZE x SIZE field of smooth noise around mid grey that tiles: a small random grid,
    repeated 3 x 3, scaled up, and the middle tile cut out."""
    cell = Image.new("L", (cols, rows))
    cell.putdata([rng.randrange(256) for _ in range(cols * rows)])
    sheet = Image.new("L", (cols * 3, rows * 3))
    for ty in range(3):
        for tx in range(3):
            sheet.paste(cell, (tx * cols, ty * rows))
    sheet = sheet.resize((SIZE * 3, SIZE * 3), Image.BICUBIC)
    if blur:
        sheet = sheet.filter(ImageFilter.GaussianBlur(blur))
    return sheet.crop((SIZE, SIZE, SIZE * 2, SIZE * 2))


def grain(seed: int, sigma: float) -> Image.Image:
    """Per-pixel noise around mid grey; too fine for a seam to show."""
    rng = random.Random(seed)
    image = Image.new("L", (SIZE, SIZE))
    image.putdata([max(0, min(255, round(rng.gauss(128, sigma)))) for _ in range(SIZE * SIZE)])
    return image


def compose(base: tuple[int, int, int], layers: list[tuple[Image.Image, float]]) -> Image.Image:
    """`base` shifted per pixel by each layer's distance from mid grey times its weight."""
    shift = [0.0] * (SIZE * SIZE)
    for layer, weight in layers:
        for k, value in enumerate(layer.tobytes()):
            shift[k] += (value - 128) * weight
    out = Image.new("RGB", (SIZE, SIZE))
    out.putdata([tuple(max(0, min(255, round(c + s))) for c in base) for s in shift])
    return out


def ink() -> Image.Image:
    rng = random.Random(1907)
    return compose(
        (25, 27, 27),
        [
            (field(rng, 6, 6), 0.035),  # broad clouding
            (field(rng, 5, 96, 0.6), 0.045),  # the slate's cleavage: long, thin, level streaks
            (field(rng, 20, 256, 0.4), 0.02),
            (grain(11, 26), 0.045),
        ],
    )


def washi() -> Image.Image:
    rng = random.Random(1612)
    paper = compose((235, 230, 219), [(field(rng, 5, 5), 0.03), (field(rng, 28, 28, 1.2), 0.025), (grain(7, 24), 0.04)])
    # Fibres: short, slightly bent strands, lighter or darker than the sheet, drawn on a 3 x 3
    # sheet so those crossing an edge continue on the other side.
    lighter = Image.new("L", (SIZE * 3, SIZE * 3), 0)
    darker = Image.new("L", (SIZE * 3, SIZE * 3), 0)
    for _ in range(360):
        target = ImageDraw.Draw(lighter if rng.random() < 0.55 else darker)
        x, y = rng.uniform(0, SIZE), rng.uniform(0, SIZE)
        heading = rng.uniform(0, 360)
        strength = rng.randrange(60, 170)
        points = [(x, y)]
        for _ in range(rng.randrange(5, 12)):
            heading += rng.uniform(-22, 22)
            step = rng.uniform(4, 9)
            x += math.cos(math.radians(heading)) * step
            y += math.sin(math.radians(heading)) * step
            points.append((x, y))
        for ty in range(3):
            for tx in range(3):
                target.line([(px + tx * SIZE, py + ty * SIZE) for px, py in points], fill=strength, width=1)
    box = (SIZE, SIZE, SIZE * 2, SIZE * 2)
    lighter = lighter.filter(ImageFilter.GaussianBlur(0.55)).crop(box)
    darker = darker.filter(ImageFilter.GaussianBlur(0.55)).crop(box)
    paper = Image.composite(Image.new("RGB", (SIZE, SIZE), (250, 247, 239)), paper, lighter.point(lambda v: v * 0.42))
    return Image.composite(Image.new("RGB", (SIZE, SIZE), (196, 187, 168)), paper, darker.point(lambda v: v * 0.3))


if __name__ == "__main__":
    ink().save(OUT / "stage-ink.jpg", quality=90, optimize=True)
    washi().save(OUT / "stage-washi.jpg", quality=90, optimize=True)
    for name in ("stage-ink.jpg", "stage-washi.jpg"):
        print(name, (OUT / name).stat().st_size, "bytes")
