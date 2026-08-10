#!/usr/bin/env python3
"""Small dependency-free pixel sprite renderer used by Sprite Studio's Codex agent."""

import json
import re
import struct
import sys
import zlib
from datetime import datetime, timezone
from pathlib import Path
import shutil


def fail(message):
    raise SystemExit(f"sprite_tool: {message}")


def slug(value):
    value = re.sub(r"[^a-zA-Z0-9_-]+", "_", str(value).strip()).strip("_-").lower()
    return value or "sprite"


def color(value):
    if value is None or value == "transparent":
        return (0, 0, 0, 0)
    value = str(value).lstrip("#")
    if len(value) == 3:
        value = "".join(part * 2 for part in value) + "ff"
    elif len(value) == 6:
        value += "ff"
    if len(value) != 8 or not re.fullmatch(r"[0-9a-fA-F]{8}", value):
        fail(f"invalid color #{value}")
    return tuple(int(value[index:index + 2], 16) for index in range(0, 8, 2))


class Canvas:
    def __init__(self, width, height, background=None):
        self.width = width
        self.height = height
        self.data = bytearray(color(background) * (width * height))

    def pixel(self, x, y, fill):
        x, y = int(round(x)), int(round(y))
        if 0 <= x < self.width and 0 <= y < self.height:
            offset = (y * self.width + x) * 4
            self.data[offset:offset + 4] = bytes(fill)

    def rect(self, x, y, width, height, fill):
        for py in range(int(y), int(y + height)):
            for px in range(int(x), int(x + width)):
                self.pixel(px, py, fill)

    def line(self, x0, y0, x1, y1, fill, thickness=1):
        x0, y0, x1, y1 = map(lambda item: int(round(item)), (x0, y0, x1, y1))
        dx, sx = abs(x1 - x0), 1 if x0 < x1 else -1
        dy, sy = -abs(y1 - y0), 1 if y0 < y1 else -1
        error = dx + dy
        radius = max(0, int(thickness) // 2)
        while True:
            self.rect(x0 - radius, y0 - radius, radius * 2 + 1, radius * 2 + 1, fill)
            if x0 == x1 and y0 == y1:
                break
            doubled = 2 * error
            if doubled >= dy:
                error += dy
                x0 += sx
            if doubled <= dx:
                error += dx
                y0 += sy

    def ellipse(self, x, y, width, height, fill):
        if width <= 0 or height <= 0:
            return
        cx, cy = x + (width - 1) / 2, y + (height - 1) / 2
        rx, ry = max(width / 2, 0.5), max(height / 2, 0.5)
        for py in range(int(y), int(y + height)):
            for px in range(int(x), int(x + width)):
                if ((px - cx) / rx) ** 2 + ((py - cy) / ry) ** 2 <= 1:
                    self.pixel(px, py, fill)

    def polygon(self, points, fill):
        points = [(float(point[0]), float(point[1])) for point in points]
        if len(points) < 3:
            return
        min_y = max(0, int(min(point[1] for point in points)))
        max_y = min(self.height - 1, int(max(point[1] for point in points)))
        for y in range(min_y, max_y + 1):
            intersections = []
            scan_y = y + 0.5
            for index, first in enumerate(points):
                second = points[(index + 1) % len(points)]
                if (first[1] <= scan_y < second[1]) or (second[1] <= scan_y < first[1]):
                    intersections.append(first[0] + (scan_y - first[1]) * (second[0] - first[0]) / (second[1] - first[1]))
            intersections.sort()
            for index in range(0, len(intersections) - 1, 2):
                start, end = int(intersections[index] + 0.5), int(intersections[index + 1] + 0.5)
                self.rect(start, y, max(1, end - start), 1, fill)

    def command(self, command, palette):
        kind = command.get("type")
        fill = color(palette.get(str(command.get("color")), command.get("color")))
        if kind == "pixel":
            self.pixel(command["x"], command["y"], fill)
        elif kind == "rect":
            self.rect(command["x"], command["y"], command["w"], command["h"], fill)
        elif kind == "line":
            self.line(command["x1"], command["y1"], command["x2"], command["y2"], fill, command.get("thickness", 1))
        elif kind == "ellipse":
            self.ellipse(command["x"], command["y"], command["w"], command["h"], fill)
        elif kind == "polygon":
            self.polygon(command["points"], fill)
        else:
            fail(f"unsupported drawing command {kind!r}")

    def pixel_rows(self, rows, palette):
        for y, row in enumerate(rows):
            for x, key in enumerate(row):
                if key not in (".", " "):
                    self.pixel(x, y, color(palette.get(key, key)))

    def save_png(self, destination):
        raw = b"".join(b"\x00" + bytes(self.data[y * self.width * 4:(y + 1) * self.width * 4]) for y in range(self.height))

        def chunk(kind, payload):
            return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)

        png = b"\x89PNG\r\n\x1a\n"
        png += chunk(b"IHDR", struct.pack(">IIBBBBB", self.width, self.height, 8, 6, 0, 0, 0))
        png += chunk(b"IDAT", zlib.compress(raw, 9))
        png += chunk(b"IEND", b"")
        destination.write_bytes(png)


def main():
    if len(sys.argv) != 2:
        fail("usage: python3 .sprite-studio/sprite_tool.py SPEC.json")
    workspace = Path.cwd().resolve()
    spec_path = Path(sys.argv[1])
    if not spec_path.is_absolute():
        spec_path = workspace / spec_path
    spec = json.loads(spec_path.read_text())
    category = slug(spec.get("category", "characters"))
    if category not in {"characters", "creatures", "terrain", "props", "effects"}:
        fail("category must be characters, creatures, terrain, props, or effects")
    name = slug(spec.get("name", "sprite"))
    animation_name = spec.get("animation", name.replace("_", " ").title())
    width, height = map(int, spec.get("size", [32, 32]))
    if not (8 <= width <= 512 and 8 <= height <= 512):
        fail("sprite size must be between 8 and 512 pixels")
    frames = spec.get("frames") or [{"name": name, "commands": spec.get("commands", [])}]
    if not (1 <= len(frames) <= 24):
        fail("a generation must contain between 1 and 24 frames")
    destination = workspace / "assets" / category
    destination.mkdir(parents=True, exist_ok=True)
    manifest_path = workspace / ".sprite-studio" / "last-generation.json"
    if manifest_path.is_file():
        try:
            previous = json.loads(manifest_path.read_text())
            if previous.get("name") == animation_name and previous.get("category") == category:
                archive = workspace / ".sprite-studio" / "versions" / name / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
                for relative in previous.get("files", []):
                    old_file = workspace / relative
                    if old_file.is_file() and old_file.resolve().is_relative_to((workspace / "assets").resolve()):
                        archive.mkdir(parents=True, exist_ok=True)
                        shutil.move(str(old_file), archive / old_file.name)
        except (OSError, ValueError, TypeError):
            pass
    frame_names = []
    for index, frame in enumerate(frames):
        frame_name = slug(frame.get("name", f"{name}_{index + 1:02d}"))
        if len(frames) > 1 and not re.search(r"\d+$", frame_name):
            frame_name = f"{frame_name}_{index + 1:02d}"
        frame_names.append(frame_name)
    version = 1
    while True:
        suffix = "" if version == 1 else f"_v{version}"
        if not any((destination / f"{frame_name}{suffix}.png").exists() for frame_name in frame_names):
            break
        version += 1
    palette = spec.get("palette", {})
    files = []
    for index, frame in enumerate(frames):
        frame_name = frame_names[index]
        output = destination / f"{frame_name}{suffix}.png"
        canvas = Canvas(width, height, spec.get("background"))
        if frame.get("pixels"):
            canvas.pixel_rows(frame["pixels"], palette)
        for command in frame.get("commands", []):
            canvas.command(command, palette)
        canvas.save_png(output)
        files.append(str(output.relative_to(workspace)))
    manifest = {
        "name": animation_name,
        "category": category,
        "fps": max(1, min(60, int(spec.get("fps", 8)))),
        "files": files,
        "generatedAt": datetime.now(timezone.utc).isoformat(),
    }
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps(manifest))


if __name__ == "__main__":
    main()
