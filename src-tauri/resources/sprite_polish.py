#!/usr/bin/env python3
"""Normalize one ImageGen frame repair back into a Sprite Studio rig animation."""

import argparse
import hashlib
import json
import math
import shutil
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

from sprite_rig import load_png
from sprite_tool import Canvas


def fail(message):
    raise SystemExit(f"sprite_polish: {message}")


def safe_existing(workspace, value):
    path = (workspace / value).resolve() if not Path(value).is_absolute() else Path(value).resolve()
    roots = ((workspace / "assets").resolve(), (workspace / ".sprite-studio").resolve())
    if not path.is_file() or not any(path.is_relative_to(root) for root in roots):
        fail(f"input must be an existing file under assets/ or .sprite-studio/: {value}")
    return path


def safe_output(workspace, value):
    path = (workspace / value).resolve() if not Path(value).is_absolute() else Path(value).resolve()
    roots = ((workspace / "assets").resolve(), (workspace / ".sprite-studio").resolve())
    if not any(path.is_relative_to(root) for root in roots):
        fail("output must stay under assets/ or .sprite-studio/")
    return path


def pixel(rgba, width, x, y):
    offset = (y * width + x) * 4
    return tuple(rgba[offset:offset + 4])


def alpha_bounds(rgba, width, height, visible):
    points = [(x, y) for y in range(height) for x in range(width) if visible(pixel(rgba, width, x, y))]
    if not points:
        fail("image contains no usable foreground pixels")
    return min(x for x, _ in points), min(y for _, y in points), max(x for x, _ in points) + 1, max(y for _, y in points) + 1


def nearest_palette(rgb, palette):
    return min(palette, key=lambda color: sum((int(rgb[index]) - color[index]) ** 2 for index in range(3)))


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--master", required=True)
    parser.add_argument("--rough", required=True)
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--report")
    parser.add_argument(
        "--region",
        help="optional rough-canvas repair box x,y,width,height; pixels outside stay byte-identical to the rough frame",
    )
    parser.add_argument("--key-threshold", type=float, default=72.0)
    args = parser.parse_args()

    workspace = Path.cwd().resolve()
    master_path = safe_existing(workspace, args.master)
    rough_path = safe_existing(workspace, args.rough)
    input_path = safe_existing(workspace, args.input)
    output_path = safe_output(workspace, args.output)
    report_path = safe_output(workspace, args.report) if args.report else None

    master_width, master_height, master = load_png(master_path)
    width, height, rough = load_png(rough_path)
    input_width, input_height, generated = load_png(input_path)
    if (master_width, master_height) != (width, height):
        fail("master and rough frame must use the same logical canvas")
    if input_width > 4096 or input_height > 4096:
        fail("ImageGen repair exceeds the 4096px safety limit")

    repair_region = None
    if args.region:
        try:
            left, top, region_width, region_height = [int(value.strip()) for value in args.region.split(",")]
        except (TypeError, ValueError):
            fail("region must be four integers: x,y,width,height")
        if region_width <= 0 or region_height <= 0:
            fail("region width and height must be positive")
        if left < 0 or top < 0 or left + region_width > width or top + region_height > height:
            fail("region must stay inside the logical canvas")
        if region_width * region_height > width * height * 0.5:
            fail("region may cover at most half of the logical canvas; use Full redraw for larger changes")
        repair_region = (left, top, left + region_width, top + region_height)

    palette_counts = Counter(
        pixel(master, master_width, x, y)[:3]
        for y in range(master_height)
        for x in range(master_width)
        if pixel(master, master_width, x, y)[3] >= 128
    )
    palette = [color for color, _ in palette_counts.most_common(64)]
    if not palette:
        fail("approved master has no opaque palette")

    rough_box = alpha_bounds(rough, width, height, lambda rgba: rgba[3] >= 64)
    corners = [
        pixel(generated, input_width, 0, 0),
        pixel(generated, input_width, input_width - 1, 0),
        pixel(generated, input_width, 0, input_height - 1),
        pixel(generated, input_width, input_width - 1, input_height - 1),
    ]
    has_native_alpha = sum(corner[3] for corner in corners) <= 64
    key = tuple(round(sum(corner[channel] for corner in corners) / 4) for channel in range(3))
    threshold_squared = args.key_threshold ** 2

    def generated_visible(rgba):
        if has_native_alpha:
            return rgba[3] >= 64
        return sum((rgba[channel] - key[channel]) ** 2 for channel in range(3)) > threshold_squared

    source_box = alpha_bounds(generated, input_width, input_height, generated_visible)
    source_width, source_height = source_box[2] - source_box[0], source_box[3] - source_box[1]
    target_width, target_height = rough_box[2] - rough_box[0], rough_box[3] - rough_box[1]
    scale = min(target_width / source_width, target_height / source_height)
    fitted_width = max(1, round(source_width * scale))
    fitted_height = max(1, round(source_height * scale))
    destination_left = round((rough_box[0] + rough_box[2] - fitted_width) / 2)
    destination_top = round((rough_box[1] + rough_box[3] - fitted_height) / 2)

    canvas = Canvas(width, height)
    written = 0
    output_points = set()
    for destination_y in range(destination_top, destination_top + fitted_height):
        for destination_x in range(destination_left, destination_left + fitted_width):
            source_x = source_box[0] + min(source_width - 1, int((destination_x - destination_left) * source_width / fitted_width))
            source_y = source_box[1] + min(source_height - 1, int((destination_y - destination_top) * source_height / fitted_height))
            rgba = pixel(generated, input_width, source_x, source_y)
            if not generated_visible(rgba):
                continue
            mapped = nearest_palette(rgba[:3], palette)
            canvas.pixel(destination_x, destination_y, (*mapped, 255))
            written += 1
            output_points.add((destination_x, destination_y))
    rough_points = {
        (x, y)
        for y in range(height)
        for x in range(width)
        if pixel(rough, width, x, y)[3] >= 64
    }
    candidate_coverage_ratio = written / max(1, len(rough_points))
    if not 0.55 <= candidate_coverage_ratio <= 1.65:
        fail(f"polished silhouette coverage drifted too far from the rig frame ({candidate_coverage_ratio:.2f}x)")
    candidate_silhouette_iou = len(output_points & rough_points) / max(1, len(output_points | rough_points))
    if candidate_silhouette_iou < 0.70:
        fail(
            "polished silhouette no longer follows the deterministic rough pose "
            f"(IoU {candidate_silhouette_iou:.3f}, minimum 0.700)"
        )

    final_canvas = canvas
    changed_pixels = sum(
        pixel(canvas.data, width, x, y) != pixel(rough, width, x, y)
        for y in range(height)
        for x in range(width)
    )
    if repair_region:
        left, top, right, bottom = repair_region
        final_canvas = Canvas(width, height)
        final_canvas.data = bytearray(rough)
        changed_pixels = 0
        for y in range(top, bottom):
            for x in range(left, right):
                candidate = pixel(canvas.data, width, x, y)
                original = pixel(rough, width, x, y)
                final_canvas.pixel(x, y, candidate)
                changed_pixels += candidate != original
        if changed_pixels == 0:
            fail("repair region did not change any rough-frame pixels")

    final_points = {
        (x, y)
        for y in range(height)
        for x in range(width)
        if pixel(final_canvas.data, width, x, y)[3] >= 64
    }
    coverage_ratio = len(final_points) / max(1, len(rough_points))
    silhouette_iou = len(final_points & rough_points) / max(1, len(final_points | rough_points))
    if repair_region and silhouette_iou < 0.85:
        fail(
            "regional repair changed too much of the deterministic silhouette "
            f"(IoU {silhouette_iou:.3f}, minimum 0.850)"
        )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    archived = None
    if output_path.exists():
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        archive = workspace / ".sprite-studio" / "versions" / "polish" / output_path.stem / stamp
        archive.mkdir(parents=True, exist_ok=True)
        archived = archive / output_path.name
        shutil.copy2(output_path, archived)
    final_canvas.save_png(output_path)

    result = {
        "valid": True,
        "master": str(master_path.relative_to(workspace)),
        "roughHash": sha256(rough_path if rough_path != output_path else archived),
        "input": str(input_path.relative_to(workspace)),
        "inputHash": sha256(input_path),
        "output": str(output_path.relative_to(workspace)),
        "outputHash": sha256(output_path),
        "canvas": [width, height],
        "paletteColors": len(palette),
        "coverageRatio": round(coverage_ratio, 4),
        "silhouetteIoU": round(silhouette_iou, 4),
        "candidateCoverageRatio": round(candidate_coverage_ratio, 4),
        "candidateSilhouetteIoU": round(candidate_silhouette_iou, 4),
        "repairRegion": [
            repair_region[0],
            repair_region[1],
            repair_region[2] - repair_region[0],
            repair_region[3] - repair_region[1],
        ] if repair_region else None,
        "changedPixels": changed_pixels,
        "outsideRegionUnchanged": bool(repair_region),
        "archivedRough": str(archived.relative_to(workspace)) if archived else None,
        "backgroundMode": "native-alpha" if has_native_alpha else "corner-chroma",
        "generatedAt": datetime.now(timezone.utc).isoformat(),
    }
    if report_path:
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
