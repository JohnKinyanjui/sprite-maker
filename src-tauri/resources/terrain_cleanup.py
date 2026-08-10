#!/usr/bin/env python3
"""Remove accidental magenta chroma fringe next to transparency in terrain atlases."""

import json
import shutil
import subprocess
import sys
from pathlib import Path


def fail(message):
    raise SystemExit(f"terrain_cleanup: {message}")


def is_magenta(pixel):
    red, green, blue, alpha = pixel
    return alpha > 0 and red > 120 and blue > 120 and green < min(red, blue) * 0.65


def clean_with_pillow(source, destination):
    from PIL import Image

    image = Image.open(source).convert("RGBA")
    pixels = image.load()
    width, height = image.size
    candidates = []
    for y in range(height):
        for x in range(width):
            if not is_magenta(pixels[x, y]):
                continue
            near_transparency = any(
                pixels[near_x, near_y][3] == 0
                for near_y in range(max(0, y - 2), min(height, y + 3))
                for near_x in range(max(0, x - 2), min(width, x + 3))
            )
            if near_transparency:
                candidates.append((x, y))
    for x, y in candidates:
        pixels[x, y] = (0, 0, 0, 0)
    image.save(destination, format="PNG", optimize=True)
    return len(candidates)


def clean_with_imagemagick(source, destination):
    executable = shutil.which("magick")
    if not executable:
        fail("Pillow or ImageMagick is required for terrain fringe cleanup")
    expression = "(a>0&&r>0.470588&&b>0.470588&&g<min(r,b)*0.65)?0:u"
    subprocess.run(
        [executable, str(source), "-channel", "RGBA", "-fx", expression, str(destination)],
        check=True,
    )
    return None


def main():
    if len(sys.argv) not in (2, 3):
        fail("usage: python3 terrain_cleanup.py INPUT.png [OUTPUT.png]")
    source = Path(sys.argv[1]).expanduser().resolve()
    destination = Path(sys.argv[2]).expanduser().resolve() if len(sys.argv) == 3 else source
    if not source.is_file() or source.suffix.lower() != ".png":
        fail("input must be an existing PNG")
    temporary = destination.with_name(f".{destination.name}.terrain-cleanup.tmp.png")
    try:
        try:
            removed = clean_with_pillow(source, temporary)
        except ImportError:
            removed = clean_with_imagemagick(source, temporary)
        destination.parent.mkdir(parents=True, exist_ok=True)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)
    print(json.dumps({"path": str(destination), "removedFringePixels": removed}))


if __name__ == "__main__":
    main()
