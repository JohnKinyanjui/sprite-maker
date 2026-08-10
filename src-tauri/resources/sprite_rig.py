#!/usr/bin/env python3
"""Dependency-free layered pixel-rig renderer for Sprite Studio.

ImageGen (or an imported asset) supplies one transparent RGBA master. A JSON rig
selects stable pixel regions and applies deterministic nearest-neighbour
transforms to those same pixels for every animation frame.
"""

import json
import hashlib
import math
import shutil
import struct
import sys
import zlib
from datetime import datetime, timezone
from pathlib import Path

from sprite_tool import Canvas, color, slug


def fail(message):
    raise SystemExit(f"sprite_rig: {message}")


def load_png(path):
    payload = path.read_bytes()
    if payload[:8] != b"\x89PNG\r\n\x1a\n":
        fail(f"source is not a PNG: {path}")
    offset = 8
    width = height = bit_depth = color_type = interlace = None
    compressed = bytearray()
    palette = b""
    transparency = b""
    while offset + 12 <= len(payload):
        length = struct.unpack(">I", payload[offset:offset + 4])[0]
        kind = payload[offset + 4:offset + 8]
        chunk = payload[offset + 8:offset + 8 + length]
        offset += 12 + length
        if kind == b"IHDR":
            width, height, bit_depth, color_type, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
        elif kind == b"IDAT":
            compressed.extend(chunk)
        elif kind == b"PLTE":
            palette = chunk
        elif kind == b"tRNS":
            transparency = chunk
        elif kind == b"IEND":
            break
    if not width or not height or bit_depth != 8 or color_type not in {2, 3, 6} or interlace != 0:
        fail("source PNG must be non-interlaced 8-bit RGB, RGBA, or indexed color")
    if color_type == 3 and (not palette or len(palette) % 3):
        fail("indexed source PNG is missing a valid palette")
    channels = 4 if color_type == 6 else (3 if color_type == 2 else 1)
    stride = width * channels
    raw = zlib.decompress(bytes(compressed))
    expected = height * (stride + 1)
    if len(raw) != expected:
        fail("source PNG has an unexpected scanline layout")
    rows = []
    cursor = 0
    previous = bytearray(stride)
    for _ in range(height):
        filter_type = raw[cursor]
        cursor += 1
        scanline = bytearray(raw[cursor:cursor + stride])
        cursor += stride
        reconstructed = bytearray(stride)
        for index, value in enumerate(scanline):
            left = reconstructed[index - channels] if index >= channels else 0
            up = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_type == 0:
                predictor = 0
            elif filter_type == 1:
                predictor = left
            elif filter_type == 2:
                predictor = up
            elif filter_type == 3:
                predictor = (left + up) // 2
            elif filter_type == 4:
                estimate = left + up - upper_left
                distances = (abs(estimate - left), abs(estimate - up), abs(estimate - upper_left))
                predictor = (left, up, upper_left)[distances.index(min(distances))]
            else:
                fail(f"unsupported PNG filter {filter_type}")
            reconstructed[index] = (value + predictor) & 255
        rows.append(reconstructed)
        previous = reconstructed
    rgba = bytearray(width * height * 4)
    for y, row in enumerate(rows):
        for x in range(width):
            source_offset = x * channels
            target_offset = (y * width + x) * 4
            if color_type == 3:
                palette_index = row[source_offset]
                palette_offset = palette_index * 3
                if palette_offset + 3 > len(palette):
                    fail("indexed source PNG refers outside its palette")
                rgba[target_offset:target_offset + 3] = palette[palette_offset:palette_offset + 3]
                rgba[target_offset + 3] = transparency[palette_index] if palette_index < len(transparency) else 255
            else:
                rgba[target_offset:target_offset + 3] = row[source_offset:source_offset + 3]
                rgba[target_offset + 3] = row[source_offset + 3] if channels == 4 else 255
    return width, height, rgba


def inside_polygon(x, y, points):
    inside = False
    previous = points[-1]
    for current in points:
        if (current[1] > y) != (previous[1] > y):
            crossing = (previous[0] - current[0]) * (y - current[1]) / (previous[1] - current[1]) + current[0]
            if x < crossing:
                inside = not inside
        previous = current
    return inside


def mask_contains(mask, x, y):
    center_x, center_y = x + 0.5, y + 0.5
    if "rect" in mask:
        left, top, width, height = map(float, mask["rect"])
        return left <= center_x < left + width and top <= center_y < top + height
    if "polygon" in mask:
        points = [(float(point[0]), float(point[1])) for point in mask["polygon"]]
        return len(points) >= 3 and inside_polygon(center_x, center_y, points)
    fail("every rig part requires a rect or polygon mask")


def layer_from_mask(source, width, height, mask):
    layer = bytearray(len(source))
    for y in range(height):
        for x in range(width):
            if mask_contains(mask, x, y):
                offset = (y * width + x) * 4
                layer[offset:offset + 4] = source[offset:offset + 4]
    return layer


def clear_mask(layer, width, height, mask):
    for y in range(height):
        for x in range(width):
            if mask_contains(mask, x, y):
                offset = (y * width + x) * 4
                layer[offset:offset + 4] = b"\x00\x00\x00\x00"


def transformed(layer, width, height, pivot, transform, root):
    result = bytearray(len(layer))
    dx = float(transform.get("dx", 0)) + float(root.get("dx", 0))
    dy = float(transform.get("dy", 0)) + float(root.get("dy", 0))
    angle = math.radians(float(transform.get("rotate", 0)))
    scale_x = float(transform.get("scaleX", 1))
    scale_y = float(transform.get("scaleY", 1))
    if scale_x == 0 or scale_y == 0:
        fail("rig scale cannot be zero")
    cosine, sine = math.cos(angle), math.sin(angle)
    pivot_x, pivot_y = map(float, pivot)
    for destination_y in range(height):
        for destination_x in range(width):
            relative_x = destination_x + 0.5 - dx - pivot_x
            relative_y = destination_y + 0.5 - dy - pivot_y
            rotated_x = cosine * relative_x + sine * relative_y
            rotated_y = -sine * relative_x + cosine * relative_y
            source_x = int(round(rotated_x / scale_x + pivot_x - 0.5))
            source_y = int(round(rotated_y / scale_y + pivot_y - 0.5))
            if 0 <= source_x < width and 0 <= source_y < height:
                source_offset = (source_y * width + source_x) * 4
                if layer[source_offset + 3]:
                    target_offset = (destination_y * width + destination_x) * 4
                    result[target_offset:target_offset + 4] = layer[source_offset:source_offset + 4]
    return result


def composite(destination, source):
    for offset in range(0, len(source), 4):
        alpha = source[offset + 3]
        if alpha == 0:
            continue
        if alpha == 255:
            destination[offset:offset + 4] = source[offset:offset + 4]
            continue
        inverse = 255 - alpha
        destination_alpha = destination[offset + 3]
        output_alpha = alpha + destination_alpha * inverse // 255
        if output_alpha == 0:
            continue
        for channel in range(3):
            numerator = source[offset + channel] * alpha + destination[offset + channel] * destination_alpha * inverse // 255
            destination[offset + channel] = numerator // output_alpha
        destination[offset + 3] = output_alpha


def safe_source(workspace, relative):
    path = (workspace / relative).resolve()
    allowed = [(workspace / "assets").resolve(), (workspace / ".sprite-studio").resolve()]
    if not path.is_file() or not any(path.is_relative_to(root) for root in allowed):
        fail("source must be an existing PNG under assets/ or .sprite-studio/")
    return path


def finite_number(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        fail(f"{label} must be a finite number")
    return float(value)


def validate_mask(mask, width, height, label):
    kinds = [kind for kind in ("rect", "polygon") if kind in mask]
    if len(kinds) != 1:
        fail(f"{label} requires exactly one rect or polygon mask")
    if kinds[0] == "rect":
        values = mask["rect"]
        if not isinstance(values, list) or len(values) != 4:
            fail(f"{label} rect must be [x,y,width,height]")
        left, top, mask_width, mask_height = [finite_number(value, f"{label} rect") for value in values]
        if mask_width <= 0 or mask_height <= 0:
            fail(f"{label} rect width and height must be positive")
        if left < 0 or top < 0 or left + mask_width > width or top + mask_height > height:
            fail(f"{label} rect extends outside the locked master")
    else:
        points = mask["polygon"]
        if not isinstance(points, list) or len(points) < 3:
            fail(f"{label} polygon requires at least three points")
        for point in points:
            if not isinstance(point, list) or len(point) != 2:
                fail(f"{label} polygon points must be [x,y]")
            x = finite_number(point[0], f"{label} polygon x")
            y = finite_number(point[1], f"{label} polygon y")
            if x < 0 or y < 0 or x > width or y > height:
                fail(f"{label} polygon extends outside the locked master")


def validate_transform(transform, label, root=False):
    if not isinstance(transform, dict):
        fail(f"{label} must be an object")
    allowed = {"dx", "dy"} if root else {"dx", "dy", "rotate", "scaleX", "scaleY"}
    unexpected = set(transform) - allowed
    if unexpected:
        fail(f"{label} has unsupported keys: {', '.join(sorted(unexpected))}")
    for key, value in transform.items():
        number = finite_number(value, f"{label}.{key}")
        if key in {"scaleX", "scaleY"} and number == 0:
            fail(f"{label}.{key} cannot be zero")


def transform_has_motion(transform):
    """Return whether a part transform changes its source pose."""
    return any(
        abs(float(transform.get(key, default)) - default) > 1e-6
        for key, default in (("dx", 0), ("dy", 0), ("rotate", 0), ("scaleX", 1), ("scaleY", 1))
    )


def validate_articulated_motion(spec, names, frames):
    """Reject locomotion rigs that merely translate one rigid sprite."""
    proposal = spec.get("proposal", {})
    if not isinstance(proposal, dict):
        proposal = {}
    morphology = str(proposal.get("morphologyTag", "")).strip().lower()
    motion_text = " ".join((
        str(spec.get("name", "")),
        str(proposal.get("motionIntent", "")),
    )).lower()
    locomotion_verbs = {
        "walk", "walking", "run", "running", "sprint", "gallop", "trot",
        "hop", "hopping", "bound", "bounding", "pounce", "crawl", "slither",
        "swim", "fly", "flap", "takeoff", "take-off",
    }
    if morphology not in {
        "biped", "quadruped", "hexapod", "segmented-many-leg", "serpentine", "winged"
    } or not any(verb in motion_text for verb in locomotion_verbs):
        return

    moved_parts = {
        name
        for frame in frames
        for name, transform in frame.get("transforms", {}).items()
        if name != "base" and transform_has_motion(transform)
    }
    if len(moved_parts) < 2:
        fail(
            "articulated locomotion cannot be root-only: animate at least two anatomical "
            "parts independently instead of lifting or sliding the complete sprite"
        )

    if morphology == "quadruped" and any(
        verb in motion_text for verb in {"hop", "hopping", "bound", "bounding", "pounce"}
    ):
        normalized = {name.replace("-", "_").lower() for name in names}
        has_hind = any(any(token in name for token in ("hind", "rear", "haunch", "thigh")) for name in normalized)
        has_fore = any(any(token in name for token in ("fore", "front", "shoulder")) for name in normalized)
        if not has_hind or not has_fore or len(moved_parts) < 3:
            fail(
                "a quadruped hop requires independently animated hindlimb and forelimb masks "
                "plus at least one additional articulated body part; whole-body lift is not a hop"
            )


def validate_rig(spec, source_path, width, height, source):
    frames = spec.get("frames", [])
    if not isinstance(frames, list) or not (2 <= len(frames) <= 32):
        fail("a rig animation must contain between 2 and 32 frames")
    parts = spec.get("parts", [])
    if not isinstance(parts, list) or not parts:
        fail("a rig animation requires at least one movable part")
    names = [slug(part.get("name", "")) for part in parts]
    if any(not value for value in names) or len(set(names)) != len(names):
        fail("rig part names must be unique and non-empty")

    claimed = {}
    warnings = []
    for index, part in enumerate(parts):
        label = f"part {names[index]!r}"
        mask = part.get("mask", {})
        if not isinstance(mask, dict):
            fail(f"{label} mask must be an object")
        validate_mask(mask, width, height, label)
        pivot = part.get("pivot", [width / 2, height / 2])
        if not isinstance(pivot, list) or len(pivot) != 2:
            fail(f"{label} pivot must be [x,y]")
        pivot_x = finite_number(pivot[0], f"{label} pivot x")
        pivot_y = finite_number(pivot[1], f"{label} pivot y")
        if not (0 <= pivot_x <= width and 0 <= pivot_y <= height):
            fail(f"{label} pivot lies outside the locked master")
        visible = []
        for y in range(height):
            for x in range(width):
                offset = (y * width + x) * 4
                if source[offset + 3] and mask_contains(mask, x, y):
                    visible.append((x, y))
        if not visible:
            fail(f"{label} selected no visible pixels")
        overlap = sorted({claimed[pixel] for pixel in visible if pixel in claimed})
        if overlap and not part.get("allowOverlap", False):
            warnings.append(f"{label} overlaps {', '.join(overlap)}; tighten the mask or set allowOverlap for intentional joint coverage")
        for pixel in visible:
            claimed.setdefault(pixel, names[index])

    valid_targets = set(names) | {"base"}
    for index, frame in enumerate(frames):
        if not isinstance(frame, dict):
            fail(f"frame {index + 1} must be an object")
        validate_transform(frame.get("root", {}), f"frame {index + 1} root", root=True)
        transforms = frame.get("transforms", {})
        if not isinstance(transforms, dict):
            fail(f"frame {index + 1} transforms must be an object")
        unknown = set(transforms) - valid_targets
        if unknown:
            fail(f"frame {index + 1} transforms unknown parts: {', '.join(sorted(unknown))}")
        for target, transform in transforms.items():
            validate_transform(transform, f"frame {index + 1} transform {target!r}")

    validate_articulated_motion(spec, names, frames)

    master_hash = hashlib.sha256(source_path.read_bytes()).hexdigest()
    recorded_hash = spec.get("masterHash")
    if recorded_hash and recorded_hash != master_hash:
        fail("locked master hash changed; create a new rig revision instead of rendering drifted artwork")
    spec["rigVersion"] = 1
    spec["planningMode"] = "ai-rig-deterministic-render"
    spec["masterHash"] = master_hash
    return names, frames, parts, warnings, master_hash


def main():
    validate_only = len(sys.argv) == 3 and sys.argv[1] == "--validate"
    if not (len(sys.argv) == 2 or validate_only):
        fail("usage: python3 .sprite-studio/sprite_rig.py [--validate] RIG.json")
    workspace = Path.cwd().resolve()
    spec_path = Path(sys.argv[2] if validate_only else sys.argv[1])
    if not spec_path.is_absolute():
        spec_path = workspace / spec_path
    spec = json.loads(spec_path.read_text())
    source_path = safe_source(workspace, spec.get("source", ""))
    width, height, source = load_png(source_path)
    if not (8 <= width <= 512 and 8 <= height <= 512):
        fail("source canvas must be between 8 and 512 pixels per side")
    name = slug(spec.get("name", source_path.stem))
    category = slug(spec.get("category", "characters"))
    if category not in {"characters", "creatures", "terrain", "props", "effects"}:
        fail("category must be characters, creatures, terrain, props, or effects")
    names, frames, parts, warnings, master_hash = validate_rig(spec, source_path, width, height, source)
    report = {
        "valid": True,
        "name": name,
        "source": str(source_path.relative_to(workspace)),
        "masterHash": master_hash,
        "parts": names,
        "frames": len(frames),
        "warnings": warnings,
    }
    if validate_only:
        spec_path.write_text(json.dumps(spec, indent=2) + "\n")
        print(json.dumps(report))
        return

    base = bytearray(source)
    layers = []
    for index, part in enumerate(parts):
        mask = part.get("mask", {})
        layer = layer_from_mask(source, width, height, mask)
        if not any(layer[offset + 3] for offset in range(0, len(layer), 4)):
            fail(f"part {names[index]!r} selected no visible pixels")
        if not part.get("keepInBase", False):
            clear_mask(base, width, height, mask)
        layers.append({
            "name": names[index],
            "pixels": layer,
            "pivot": part.get("pivot", [width / 2, height / 2]),
            "z": int(part.get("z", index)),
        })

    destination = workspace / "assets" / category
    destination.mkdir(parents=True, exist_ok=True)
    output_paths = [destination / f"{name}_{index + 1:02d}.png" for index in range(len(frames))]
    if any(path.exists() for path in output_paths):
        manifest_path = workspace / ".sprite-studio" / "last-generation.json"
        try:
            previous = json.loads(manifest_path.read_text())
        except (OSError, ValueError, TypeError):
            previous = {}
        expected = [str(path.relative_to(workspace)) for path in output_paths]
        if previous.get("name") != name or previous.get("category") != category or previous.get("files") != expected:
            fail("output frames exist but are not the active result for this exact rig")
        archive = workspace / ".sprite-studio" / "versions" / "rigs" / name / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        archive.mkdir(parents=True, exist_ok=True)
        for path in output_paths:
            if path.is_file():
                shutil.move(str(path), archive / path.name)
    palette = spec.get("palette", {})
    for index, frame in enumerate(frames):
        root = frame.get("root", {})
        transforms = frame.get("transforms", {})
        canvas = Canvas(width, height)
        base_pixels = transformed(base, width, height, [0, 0], transforms.get("base", {}), root)
        composite(canvas.data, base_pixels)
        for command in frame.get("underlay", []):
            canvas.command(command, palette)
        for layer in sorted(layers, key=lambda item: item["z"]):
            pixels = transformed(
                layer["pixels"], width, height, layer["pivot"],
                transforms.get(layer["name"], {}), root,
            )
            composite(canvas.data, pixels)
        for command in frame.get("overlay", []):
            canvas.command(command, palette)
        canvas.save_png(output_paths[index])

    fps = max(1, min(60, int(spec.get("fps", 8))))
    manifest = {
        "name": name,
        "category": category,
        "fps": fps,
        "files": [str(path.relative_to(workspace)) for path in output_paths],
        "rig": str((Path(".sprite-studio") / "rigs" / f"{name}.json")),
        "masterHash": master_hash,
        "planningMode": "ai-rig-deterministic-render",
        "generatedAt": datetime.now(timezone.utc).isoformat(),
    }
    metadata = workspace / ".sprite-studio"
    metadata.mkdir(parents=True, exist_ok=True)
    (metadata / "last-generation.json").write_text(json.dumps(manifest, indent=2) + "\n")
    rig_record = metadata / "rigs" / f"{name}.json"
    rig_record.parent.mkdir(parents=True, exist_ok=True)
    rig_record.write_text(json.dumps(spec, indent=2) + "\n")
    print(json.dumps(manifest))


if __name__ == "__main__":
    main()
