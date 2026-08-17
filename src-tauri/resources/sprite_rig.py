#!/usr/bin/env python3
"""Dependency-free layered pixel-rig renderer for Sprite Studio.

ImageGen (or an imported asset) supplies one transparent RGBA master. A JSON rig
selects stable pixel regions and applies deterministic nearest-neighbour
transforms to those same pixels for every animation frame.
"""

import json
import hashlib
import math
import os
import statistics
import shutil
import struct
import sys
import tempfile
import zlib
from datetime import datetime, timezone
from pathlib import Path

from sprite_tool import Canvas, color, slug


def fail(message):
    raise SystemExit(f"sprite_rig: {message}")


def reject_json_constant(value):
    raise ValueError(f"non-finite JSON constant is not allowed: {value}")


def strict_json_loads(value):
    return json.loads(value, parse_constant=reject_json_constant)


def strict_json_dumps(value, **kwargs):
    return json.dumps(value, allow_nan=False, **kwargs)


def load_png(path):
    try:
        file_size = path.stat().st_size
    except OSError as error:
        fail(f"cannot inspect source PNG: {error}")
    if file_size > 32 * 1024 * 1024:
        fail("source PNG file is too large")
    payload = path.read_bytes()
    payload_hash = hashlib.sha256(payload).hexdigest()
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
            if not (8 <= width <= 512 and 8 <= height <= 512):
                fail("source canvas must be between 8 and 512 pixels per side")
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
    expected = height * (stride + 1)
    decompressor = zlib.decompressobj()
    try:
        raw = decompressor.decompress(bytes(compressed), expected + 1)
        if len(raw) > expected or decompressor.unconsumed_tail:
            fail("source PNG expands beyond its declared canvas")
        raw += decompressor.flush(expected + 1 - len(raw))
    except zlib.error as error:
        fail(f"source PNG has invalid compressed data: {error}")
    if len(raw) > expected or not decompressor.eof:
        fail("source PNG expands beyond its declared canvas")
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
    return width, height, rgba, payload_hash


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


IDENTITY_MATRIX = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def multiply_matrix(left, right):
    """Return the affine matrix left ∘ right."""
    la, lb, lc, ld, ltx, lty = left
    ra, rb, rc, rd, rtx, rty = right
    return (
        la * ra + lc * rb,
        lb * ra + ld * rb,
        la * rc + lc * rd,
        lb * rc + ld * rd,
        la * rtx + lc * rty + ltx,
        lb * rtx + ld * rty + lty,
    )


def translation_matrix(dx, dy):
    return (1.0, 0.0, 0.0, 1.0, float(dx), float(dy))


def local_matrix(pivot, transform):
    pivot_x, pivot_y = map(float, pivot)
    dx = float(transform.get("dx", 0))
    dy = float(transform.get("dy", 0))
    angle = math.radians(float(transform.get("rotate", 0)))
    scale_x = float(transform.get("scaleX", 1))
    scale_y = float(transform.get("scaleY", 1))
    if scale_x == 0 or scale_y == 0:
        fail("rig scale cannot be zero")
    cosine, sine = math.cos(angle), math.sin(angle)
    around_origin = (
        cosine * scale_x,
        sine * scale_x,
        -sine * scale_y,
        cosine * scale_y,
        0.0,
        0.0,
    )
    return multiply_matrix(
        translation_matrix(dx + pivot_x, dy + pivot_y),
        multiply_matrix(around_origin, translation_matrix(-pivot_x, -pivot_y)),
    )


def invert_matrix(matrix):
    a, b, c, d, tx, ty = matrix
    determinant = a * d - b * c
    if abs(determinant) < 1e-9:
        fail("rig transform is not invertible")
    return (
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * ty - d * tx) / determinant,
        (b * tx - a * ty) / determinant,
    )


def apply_matrix(matrix, point):
    a, b, c, d, tx, ty = matrix
    x, y = map(float, point)
    return (a * x + c * y + tx, b * x + d * y + ty)


def matrix_rotation(matrix):
    return math.degrees(math.atan2(matrix[1], matrix[0]))


def transformed_matrix(layer, width, height, matrix):
    """Rasterize a layer through one composed affine matrix."""
    result = bytearray(len(layer))
    inverse = invert_matrix(matrix)
    for destination_y in range(height):
        for destination_x in range(width):
            source_center = apply_matrix(inverse, (destination_x + 0.5, destination_y + 0.5))
            source_x = int(round(source_center[0] - 0.5))
            source_y = int(round(source_center[1] - 0.5))
            if 0 <= source_x < width and 0 <= source_y < height:
                source_offset = (source_y * width + source_x) * 4
                if layer[source_offset + 3]:
                    target_offset = (destination_y * width + destination_x) * 4
                    result[target_offset:target_offset + 4] = layer[source_offset:source_offset + 4]
    return result


def barycentric_coordinates(point, first, second, third):
    """Return barycentric weights for point, or None for a degenerate triangle."""
    px, py = point
    ax, ay = first
    bx, by = second
    cx, cy = third
    denominator = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
    if abs(denominator) < 1e-9:
        return None
    first_weight = ((by - cy) * (px - cx) + (cx - bx) * (py - cy)) / denominator
    second_weight = ((cy - ay) * (px - cx) + (ax - cx) * (py - cy)) / denominator
    return first_weight, second_weight, 1.0 - first_weight - second_weight


def mesh_destination_vertices(mesh, matrices):
    destination_vertices = []
    for vertex, influences in zip(mesh["vertices"], mesh["weights"]):
        destination_x = 0.0
        destination_y = 0.0
        for influence in influences:
            transformed_vertex = apply_matrix(matrices[influence["bone"]], vertex)
            destination_x += transformed_vertex[0] * influence["weight"]
            destination_y += transformed_vertex[1] * influence["weight"]
        destination_vertices.append((destination_x, destination_y))
    return destination_vertices


def transformed_mesh(layer, width, height, mesh, matrices):
    """Deform a pixel layer through a weighted triangle mesh.

    The UVs are the bind-pose vertex positions. Destination pixels are mapped
    back into each source triangle and sampled nearest-neighbour, preserving
    the locked pixel palette instead of blurring it.
    """
    source_vertices = mesh["vertices"]
    destination_vertices = mesh_destination_vertices(mesh, matrices)

    result = bytearray(len(layer))
    for triangle in mesh["triangles"]:
        source_triangle = [source_vertices[index] for index in triangle]
        destination_triangle = [destination_vertices[index] for index in triangle]
        minimum_x = max(0, int(math.floor(min(point[0] for point in destination_triangle))))
        maximum_x = min(width - 1, int(math.ceil(max(point[0] for point in destination_triangle))))
        minimum_y = max(0, int(math.floor(min(point[1] for point in destination_triangle))))
        maximum_y = min(height - 1, int(math.ceil(max(point[1] for point in destination_triangle))))
        for destination_y in range(minimum_y, maximum_y + 1):
            for destination_x in range(minimum_x, maximum_x + 1):
                weights = barycentric_coordinates(
                    (destination_x + 0.5, destination_y + 0.5), *destination_triangle,
                )
                if weights is None or min(weights) < -1e-6 or max(weights) > 1.0 + 1e-6:
                    continue
                source_x_float = sum(
                    weight * point[0] for weight, point in zip(weights, source_triangle)
                )
                source_y_float = sum(
                    weight * point[1] for weight, point in zip(weights, source_triangle)
                )
                source_x = int(round(source_x_float - 0.5))
                source_y = int(round(source_y_float - 0.5))
                if not (0 <= source_x < width and 0 <= source_y < height):
                    continue
                source_offset = (source_y * width + source_x) * 4
                if not layer[source_offset + 3]:
                    continue
                target_offset = (destination_y * width + destination_x) * 4
                result[target_offset:target_offset + 4] = layer[source_offset:source_offset + 4]
    return result


def mesh_destination_point(mesh, destination_vertices, point):
    for triangle in mesh["triangles"]:
        source_triangle = [mesh["vertices"][index] for index in triangle]
        weights = barycentric_coordinates(point, *source_triangle)
        if weights is None or min(weights) < -1e-6 or max(weights) > 1.0 + 1e-6:
            continue
        destination_triangle = [destination_vertices[index] for index in triangle]
        return (
            sum(weight * vertex[0] for weight, vertex in zip(weights, destination_triangle)),
            sum(weight * vertex[1] for weight, vertex in zip(weights, destination_triangle)),
        )
    return None


def transformed(layer, width, height, pivot, transform, root):
    """Legacy flat transform wrapper retained for rigVersion 1 compatibility."""
    root_matrix = translation_matrix(root.get("dx", 0), root.get("dy", 0))
    return transformed_matrix(layer, width, height, multiply_matrix(root_matrix, local_matrix(pivot, transform)))


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
    workspace = workspace.resolve()
    path = (workspace / relative).resolve()
    allowed = [workspace / "assets", workspace / ".sprite-studio"]
    if not path.is_file() or not path.is_relative_to(workspace) or not any(
        path.is_relative_to(root) for root in allowed
    ):
        fail("source must be an existing PNG under assets/ or .sprite-studio/")
    return path


def safe_output_path(workspace, relative, label):
    """Resolve a workspace-owned write target without following it outside."""
    workspace = workspace.resolve()
    path = workspace / relative
    probe = path.parent
    while not probe.exists() and probe != workspace:
        probe = probe.parent
    if not probe.resolve().is_relative_to(workspace):
        fail(f"{label} directory must stay inside the workspace")
    if path.is_symlink():
        fail(f"{label} cannot be a symbolic link")
    return path


def ensure_regular_or_missing(path, label):
    if path.is_symlink():
        fail(f"{label} cannot be a symbolic link")
    if path.exists() and not path.is_file():
        fail(f"{label} must be a regular file")


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def copy_synced(source, destination):
    shutil.copy2(source, destination)
    with destination.open("rb") as handle:
        os.fsync(handle.fileno())
    if sha256_file(source) != sha256_file(destination):
        raise OSError(f"backup verification failed for {source.name}")


def atomic_restore(backup, target):
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{target.name}.rollback-", suffix=".tmp", dir=str(target.parent),
    )
    os.close(descriptor)
    temporary = Path(temporary_name)
    try:
        copy_synced(backup, temporary)
        os.replace(temporary, target)
    finally:
        if temporary.exists() or temporary.is_symlink():
            temporary.unlink()


def acquire_render_lock(workspace):
    lock_path = safe_output_path(
        workspace, Path(".sprite-studio") / "rig-render.lock", "rig render lock",
    )
    ensure_regular_or_missing(lock_path, "rig render lock")
    handle = lock_path.open("a+b")
    try:
        if os.name == "nt":
            import msvcrt
            if lock_path.stat().st_size == 0:
                handle.write(b"\0")
                handle.flush()
            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except (OSError, BlockingIOError):
        handle.close()
        fail("another rig render is already committing in this workspace")
    return handle


def finite_number(value, label):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        fail(f"{label} must be a finite number")
    return float(value)


def validate_color_value(value, label):
    if value is not None and not isinstance(value, str):
        fail(f"{label} must be a hexadecimal color or 'transparent'")
    try:
        color(value)
    except SystemExit:
        fail(f"{label} must be a hexadecimal color or 'transparent'")


def validate_palette(value):
    if not isinstance(value, dict):
        fail("palette must be an object")
    for key, entry in value.items():
        if not isinstance(key, str) or not key:
            fail("palette keys must be non-empty strings")
        validate_color_value(entry, f"palette color {key!r}")
    return value


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


def validate_mesh(mesh, width, height, part_names, label):
    """Validate a compact weighted deformation mesh for one pixel layer."""
    if not isinstance(mesh, dict):
        fail(f"{label} mesh must be an object")
    unexpected = set(mesh) - {"vertices", "triangles", "weights"}
    if unexpected:
        fail(f"{label} mesh has unsupported keys: {', '.join(sorted(unexpected))}")
    vertices = mesh.get("vertices")
    triangles = mesh.get("triangles")
    weights = mesh.get("weights")
    if not isinstance(vertices, list) or not (3 <= len(vertices) <= 128):
        fail(f"{label} mesh requires between 3 and 128 vertices")
    normalized_vertices = []
    for index, vertex in enumerate(vertices):
        if not isinstance(vertex, list) or len(vertex) != 2:
            fail(f"{label} mesh vertex {index + 1} must be [x,y]")
        x = finite_number(vertex[0], f"{label} mesh vertex {index + 1} x")
        y = finite_number(vertex[1], f"{label} mesh vertex {index + 1} y")
        if not (0 <= x <= width and 0 <= y <= height):
            fail(f"{label} mesh vertex {index + 1} lies outside the locked master")
        normalized_vertices.append([x, y])
    if not isinstance(triangles, list) or not triangles:
        fail(f"{label} mesh requires at least one triangle")
    normalized_triangles = []
    for index, triangle in enumerate(triangles):
        if (
            not isinstance(triangle, list)
            or len(triangle) != 3
            or any(isinstance(value, bool) or not isinstance(value, int) for value in triangle)
            or len(set(triangle)) != 3
            or any(value < 0 or value >= len(vertices) for value in triangle)
        ):
            fail(f"{label} mesh triangle {index + 1} requires three distinct vertex indexes")
        points = [normalized_vertices[value] for value in triangle]
        if barycentric_coordinates(points[0], *points) is None:
            fail(f"{label} mesh triangle {index + 1} is degenerate")
        normalized_triangles.append(triangle)
    if not isinstance(weights, list) or len(weights) != len(vertices):
        fail(f"{label} mesh requires one weight list per vertex")
    normalized_weights = []
    for vertex_index, influences in enumerate(weights):
        if not isinstance(influences, list) or not (1 <= len(influences) <= 4):
            fail(f"{label} mesh vertex {vertex_index + 1} requires 1 to 4 bone weights")
        seen = set()
        normalized_influences = []
        total = 0.0
        for influence_index, influence in enumerate(influences):
            influence_label = (
                f"{label} mesh vertex {vertex_index + 1} influence {influence_index + 1}"
            )
            if not isinstance(influence, dict) or set(influence) != {"bone", "weight"}:
                fail(f"{influence_label} requires only bone and weight")
            bone = slug(influence.get("bone", ""))
            if bone not in part_names:
                fail(f"{influence_label} refers to unknown bone {bone!r}")
            if bone in seen:
                fail(f"{label} mesh vertex {vertex_index + 1} repeats bone {bone!r}")
            seen.add(bone)
            weight = finite_number(influence.get("weight"), f"{influence_label} weight")
            if weight <= 0:
                fail(f"{influence_label} weight must be positive")
            total += weight
            normalized_influences.append({"bone": bone, "weight": weight})
        if abs(total - 1.0) > 1e-4:
            fail(f"{label} mesh vertex {vertex_index + 1} weights must sum to 1")
        normalized_weights.append(normalized_influences)
    mesh["vertices"] = normalized_vertices
    mesh["triangles"] = normalized_triangles
    mesh["weights"] = normalized_weights


def mesh_contains_point(mesh, point):
    for triangle in mesh["triangles"]:
        points = [mesh["vertices"][index] for index in triangle]
        weights = barycentric_coordinates(point, *points)
        if weights is not None and min(weights) >= -1e-6 and max(weights) <= 1.0 + 1e-6:
            return True
    return False


def validate_transform(transform, label, root=False, rig_version=1):
    if not isinstance(transform, dict):
        fail(f"{label} must be an object")
    allowed = {"dx", "dy"} if root else {"dx", "dy", "rotate", "scaleX", "scaleY"}
    if not root and rig_version >= 2:
        allowed.add("worldRotate")
    unexpected = set(transform) - allowed
    if unexpected:
        fail(f"{label} has unsupported keys: {', '.join(sorted(unexpected))}")
    if "rotate" in transform and "worldRotate" in transform:
        fail(f"{label} cannot combine rotate with worldRotate")
    for key, value in transform.items():
        number = finite_number(value, f"{label}.{key}")
        if key in {"scaleX", "scaleY"} and number == 0:
            fail(f"{label}.{key} cannot be zero")


def validate_draw_commands(commands, label, palette):
    schemas = {
        "pixel": ({"type", "x", "y", "color"}, {"x", "y"}),
        "rect": ({"type", "x", "y", "w", "h", "color"}, {"x", "y", "w", "h"}),
        "line": (
            {"type", "x1", "y1", "x2", "y2", "thickness", "color"},
            {"x1", "y1", "x2", "y2"},
        ),
        "ellipse": ({"type", "x", "y", "w", "h", "color"}, {"x", "y", "w", "h"}),
        "polygon": ({"type", "points", "color"}, set()),
    }
    for index, command in enumerate(commands):
        command_label = f"{label} command {index + 1}"
        if not isinstance(command, dict):
            fail(f"{command_label} must be an object")
        kind = command.get("type")
        if kind not in schemas:
            fail(f"{command_label} has unsupported type {kind!r}")
        allowed, required_numbers = schemas[kind]
        unexpected = set(command) - allowed
        if unexpected:
            fail(f"{command_label} has unsupported keys: {', '.join(sorted(unexpected))}")
        missing = required_numbers - set(command)
        if missing:
            fail(f"{command_label} is missing: {', '.join(sorted(missing))}")
        for key in required_numbers:
            finite_number(command[key], f"{command_label}.{key}")
        if "thickness" in command:
            if finite_number(command["thickness"], f"{command_label}.thickness") <= 0:
                fail(f"{command_label}.thickness must be positive")
        if kind in {"rect", "ellipse"} and (
            float(command["w"]) <= 0 or float(command["h"]) <= 0
        ):
            fail(f"{command_label} width and height must be positive")
        if kind == "polygon":
            points = command.get("points")
            if not isinstance(points, list) or len(points) < 3:
                fail(f"{command_label}.points requires at least three [x,y] points")
            for point_index, point in enumerate(points):
                if not isinstance(point, list) or len(point) != 2:
                    fail(f"{command_label}.points[{point_index}] must be [x,y]")
                finite_number(point[0], f"{command_label}.points[{point_index}].x")
                finite_number(point[1], f"{command_label}.points[{point_index}].y")
        raw_color = command.get("color")
        if raw_color is not None and not isinstance(raw_color, str):
            fail(f"{command_label}.color must be a palette key or hexadecimal color")
        resolved_color = palette.get(str(raw_color), raw_color)
        validate_color_value(resolved_color, f"{command_label}.color")


def transform_has_motion(transform):
    """Return whether a part transform changes its source pose."""
    return any(
        abs(float(transform.get(key, default)) - default) > 1e-6
        for key, default in (
            ("dx", 0), ("dy", 0), ("rotate", 0), ("worldRotate", 0),
            ("scaleX", 1), ("scaleY", 1),
        )
    )


def meaningfully_animated_parts(frames):
    """Return parts whose authored pose spans enough to survive pixel quantization."""
    names = {
        name
        for frame in frames
        for name in frame.get("transforms", {})
        if name != "base"
    }
    meaningful = set()
    for name in names:
        samples = [frame.get("transforms", {}).get(name, {}) for frame in frames]
        dx = [float(sample.get("dx", 0)) for sample in samples]
        dy = [float(sample.get("dy", 0)) for sample in samples]
        rotation = [float(sample.get("worldRotate", sample.get("rotate", 0))) for sample in samples]
        scale_x = [float(sample.get("scaleX", 1)) for sample in samples]
        scale_y = [float(sample.get("scaleY", 1)) for sample in samples]
        translation_span = math.hypot(max(dx) - min(dx), max(dy) - min(dy))
        rotation_span = max(rotation) - min(rotation)
        scale_span = max(max(scale_x) - min(scale_x), max(scale_y) - min(scale_y))
        if translation_span >= 1.5 or rotation_span >= 8.0 or scale_span >= 0.05:
            meaningful.add(name)
    return meaningful


def locomotion_context(spec):
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
    motion_tokens = exact_tokens(motion_text.replace(" ", "_"))
    is_locomotion = bool(motion_tokens.intersection(locomotion_verbs))
    is_grounded_walk = bool(
        motion_tokens.intersection({"walk", "walking", "trot", "crawl", "slither"})
    )
    return morphology, motion_text, is_locomotion, is_grounded_walk


def normalize_anchors(part, width, height, label):
    anchors = part.get("anchors", {})
    if not isinstance(anchors, dict) or not anchors:
        fail(f"{label} requires at least one named anchor in rigVersion 2")
    normalized = {}
    for raw_name, point in anchors.items():
        anchor_name = slug(raw_name)
        if anchor_name in normalized:
            fail(f"{label} anchor names must be unique")
        if not isinstance(point, list) or len(point) != 2:
            fail(f"{label} anchor {anchor_name!r} must be [x,y]")
        x = finite_number(point[0], f"{label} anchor {anchor_name!r} x")
        y = finite_number(point[1], f"{label} anchor {anchor_name!r} y")
        if not (0 <= x <= width and 0 <= y <= height):
            fail(f"{label} anchor {anchor_name!r} lies outside the locked master")
        normalized[anchor_name] = [x, y]
    part["anchors"] = normalized
    return normalized


RIG_PROFILE_MORPHOLOGIES = {
    "human_sprite_rig": {"biped"},
    "four_leg_sprite_rig": {"quadruped"},
    "multi_leg_sprite_rig": {"hexapod", "segmented-many-leg"},
    "serpentine_sprite_rig": {"serpentine"},
    "winged_sprite_rig": {"winged"},
}


def normalize_bone(part, label):
    bone = part.get("bone")
    if not isinstance(bone, dict) or set(bone) != {"startAnchor", "endAnchor", "radius"}:
        fail(f"{label} in rigVersion 3 requires bone with startAnchor, endAnchor, and radius")
    start_anchor = slug(bone.get("startAnchor", ""))
    end_anchor = slug(bone.get("endAnchor", ""))
    if start_anchor == end_anchor or start_anchor not in part["anchors"] or end_anchor not in part["anchors"]:
        fail(f"{label} bone endpoints must name two distinct anchors on that part")
    radius = finite_number(bone.get("radius"), f"{label} bone radius")
    if not (0.5 <= radius <= 32):
        fail(f"{label} bone radius must be between 0.5 and 32 pixels")
    part["bone"] = {
        "startAnchor": start_anchor,
        "endAnchor": end_anchor,
        "radius": radius,
    }


def validate_rig_profile(spec, parts, frames, width, height, source):
    """Validate rigVersion 3 anatomy, visible joints, and named key poses."""
    morphology, _, is_locomotion, _ = locomotion_context(spec)
    profile = str(spec.get("rigProfile", "")).strip().lower()
    if profile not in RIG_PROFILE_MORPHOLOGIES:
        fail(
            "rigVersion 3 requires rigProfile: human_sprite_rig, four_leg_sprite_rig, "
            "multi_leg_sprite_rig, serpentine_sprite_rig, or winged_sprite_rig"
        )
    if morphology not in RIG_PROFILE_MORPHOLOGIES[profile]:
        fail(f"rigProfile {profile!r} does not match morphology {morphology!r}")
    spec["rigProfile"] = profile

    part_by_name = {part["name"]: part for part in parts}
    raw_joints = spec.get("joints")
    if not isinstance(raw_joints, list) or not raw_joints:
        fail("rigVersion 3 requires an observed joints array")
    joints = []
    joint_names = set()
    for index, raw_joint in enumerate(raw_joints):
        label = f"joint {index + 1}"
        if not isinstance(raw_joint, dict) or set(raw_joint) != {
            "name", "kind", "position", "visibility", "parts",
        }:
            fail(f"{label} requires only name, kind, position, visibility, and parts")
        name = slug(raw_joint.get("name", ""))
        kind = slug(raw_joint.get("kind", ""))
        visibility = str(raw_joint.get("visibility", "")).strip().lower()
        raw_parts = raw_joint.get("parts")
        if not name or name in joint_names:
            fail("joint names must be unique and non-empty")
        joint_names.add(name)
        if visibility not in {"visible", "occluded"}:
            fail(f"{label} visibility must be 'visible' or 'occluded'")
        if (
            not isinstance(raw_parts, list) or len(raw_parts) != 2
            or any(slug(value) not in part_by_name for value in raw_parts)
        ):
            fail(f"{label} parts must name two existing anatomical parts")
        joint_parts = [slug(value) for value in raw_parts]
        if joint_parts[0] == joint_parts[1]:
            fail(f"{label} must connect two different parts")
        position = raw_joint.get("position")
        if not isinstance(position, list) or len(position) != 2:
            fail(f"{label} position must be [x,y]")
        joint_x = finite_number(position[0], f"{label} position x")
        joint_y = finite_number(position[1], f"{label} position y")
        if not (0 <= joint_x <= width and 0 <= joint_y <= height):
            fail(f"{label} position lies outside the locked master")
        normalized = {
            "name": name,
            "kind": kind,
            "position": [joint_x, joint_y],
            "visibility": visibility,
            "parts": joint_parts,
        }
        for part_name in joint_parts:
            part = part_by_name[part_name]
            distance = nearest_visible_distance(
                source, width, height, part["mask"], normalized["position"],
            )
            limit = 1.6 if visibility == "visible" else 3.0
            if distance > limit:
                fail(
                    f"joint {name!r} is marked {visibility} but sits {distance:.2f}px "
                    f"from part {part_name!r}; the anatomy is not actually segmented at that joint"
                )
        joints.append(normalized)
    spec["joints"] = joints

    required = set()
    required_visible = set()
    if profile == "human_sprite_rig" and is_locomotion:
        required = {
            "left_hip", "left_knee", "left_ankle",
            "right_hip", "right_knee", "right_ankle",
        }
        required_visible = required
    elif profile == "four_leg_sprite_rig" and is_locomotion:
        required = {
            f"{side}_{limb}_{joint}"
            for side in ("near", "far")
            for limb, joints_for_limb in (
                ("hind", ("hip", "knee", "ankle")),
                ("fore", ("shoulder", "elbow", "wrist")),
            )
            for joint in joints_for_limb
        }
        required_visible = {
            "near_hind_hip", "near_hind_knee", "near_hind_ankle",
            "near_fore_shoulder", "near_fore_elbow", "near_fore_wrist",
        }
    missing = required - joint_names
    if missing:
        fail(f"{profile} is missing observed joints: {', '.join(sorted(missing))}")
    visibility_by_name = {joint["name"]: joint["visibility"] for joint in joints}
    hidden_required = {
        name for name in required_visible if visibility_by_name.get(name) != "visible"
    }
    if hidden_required:
        fail(
            f"{profile} requires visible gameplay-side joints: "
            f"{', '.join(sorted(hidden_required))}; create a motion-ready source revision"
        )

    pose_names = []
    for index, frame in enumerate(frames):
        pose = slug(frame.get("pose", ""))
        if is_locomotion and not pose:
            fail(f"rigVersion 3 locomotion frame {index + 1} requires a named key pose")
        frame["pose"] = pose
        pose_names.append(pose)
    if is_locomotion:
        pose_tokens = set().union(*(exact_tokens(pose) for pose in pose_names))
        motion_tokens = exact_tokens(spec.get("proposal", {}).get("motionIntent", ""))
        if profile in {"human_sprite_rig", "four_leg_sprite_rig"}:
            if not {"contact", "passing"}.issubset(pose_tokens) and not motion_tokens.intersection(
                {"run", "running", "gallop", "bound", "bounding", "sprint"}
            ):
                fail("a walk cycle requires named contact and passing key poses")
        if profile == "four_leg_sprite_rig" and motion_tokens.intersection(
            {"run", "running", "gallop", "bound", "bounding", "sprint"}
        ):
            required_poses = {"hind", "fore", "extended", "gathered", "contact", "flight"}
            if not required_poses.issubset(pose_tokens):
                fail(
                    "a four-leg run requires named hind_contact, extended_flight, "
                    "fore_contact, and gathered_flight poses"
                )


def point_segment_distance(point, start, end):
    start_x, start_y = start
    end_x, end_y = end
    dx, dy = end_x - start_x, end_y - start_y
    length_squared = dx * dx + dy * dy
    if length_squared <= 1e-9:
        return point_distance(point, start)
    projection = max(0.0, min(1.0, (
        (point[0] - start_x) * dx + (point[1] - start_y) * dy
    ) / length_squared))
    return point_distance(point, (start_x + projection * dx, start_y + projection * dy))


def validate_no_residual_bone_pixels(base, width, height, parts):
    """Reject unclaimed source pixels left painted behind articulated limbs."""
    for part in parts:
        bone = part.get("bone")
        if not bone:
            continue
        start = part["anchors"][bone["startAnchor"]]
        end = part["anchors"][bone["endAnchor"]]
        residual = []
        for y in range(height):
            for x in range(width):
                offset = (y * width + x) * 4
                if not base[offset + 3]:
                    continue
                if point_segment_distance((x + 0.5, y + 0.5), start, end) <= bone["radius"]:
                    residual.append((x, y))
        if residual:
            first_x, first_y = residual[0]
            fail(
                f"part {part['name']!r} leaves {len(residual)} unclaimed source pixels "
                f"inside its bone envelope; first residual is ({first_x},{first_y}). "
                "Tighten anatomical masks instead of painting over the leftover pixels"
            )


def parse_contacts(frame, frame_index, part_by_name):
    raw_contacts = frame.get("contacts", [])
    if not isinstance(raw_contacts, list):
        fail(f"frame {frame_index + 1} contacts must be an array")
    contacts = []
    seen = set()
    for contact_index, raw_contact in enumerate(raw_contacts):
        label = f"frame {frame_index + 1} contact {contact_index + 1}"
        if isinstance(raw_contact, str) and "." in raw_contact:
            raw_part, raw_anchor = raw_contact.rsplit(".", 1)
            contact = {"part": raw_part, "anchor": raw_anchor, "state": "planted"}
        elif isinstance(raw_contact, dict):
            unexpected = set(raw_contact) - {"part", "anchor", "state"}
            if unexpected:
                fail(f"{label} has unsupported keys: {', '.join(sorted(unexpected))}")
            contact = dict(raw_contact)
        else:
            fail(f"{label} must be an object with part, anchor, and planted state")
        part_name = slug(contact.get("part", ""))
        anchor_name = slug(contact.get("anchor", ""))
        state = str(contact.get("state", "planted")).strip().lower()
        if state != "planted":
            fail(f"{label} state must be 'planted'; omit lifted contacts")
        part = part_by_name.get(part_name)
        if part is None:
            fail(f"{label} refers to unknown part {part_name!r}")
        if anchor_name not in part.get("anchors", {}):
            fail(f"{label} refers to unknown anchor {part_name}.{anchor_name}")
        key = (part_name, anchor_name)
        if key in seen:
            fail(f"{label} duplicates {part_name}.{anchor_name}")
        seen.add(key)
        contacts.append({"part": part_name, "anchor": anchor_name, "state": "planted"})
    frame["contacts"] = contacts
    return contacts


def exact_tokens(value):
    return {token for token in str(value).replace("-", "_").lower().split("_") if token}


def resolve_frame_matrices(parts, frame):
    """Resolve local part transforms into root-inclusive world matrices."""
    part_by_name = {part["name"]: part for part in parts}
    transforms = frame.get("transforms", {})
    root = frame.get("root", {})
    root_matrix = translation_matrix(root.get("dx", 0), root.get("dy", 0))
    local_world = {}
    resolving = set()

    def resolve_local_world(name):
        if name in local_world:
            return local_world[name]
        if name in resolving:
            fail(f"rig part parent cycle reaches {name!r}")
        resolving.add(name)
        part = part_by_name[name]
        parent = part.get("parent")
        parent_matrix = resolve_local_world(parent) if parent else IDENTITY_MATRIX
        transform = transforms.get(name, {})
        if "worldRotate" in transform:
            transform = dict(transform)
            transform["rotate"] = float(transform.pop("worldRotate")) - matrix_rotation(parent_matrix)
        matrix = local_matrix(part.get("pivot", [0, 0]), transform)
        if parent:
            matrix = multiply_matrix(parent_matrix, matrix)
        resolving.remove(name)
        local_world[name] = matrix
        return matrix

    matrices = {
        name: multiply_matrix(root_matrix, resolve_local_world(name))
        for name in part_by_name
    }
    matrices["base"] = multiply_matrix(
        root_matrix,
        local_matrix([0, 0], transforms.get("base", {})),
    )
    return matrices


def rotated_vector(vector, angle_degrees):
    angle = math.radians(float(angle_degrees))
    cosine, sine = math.cos(angle), math.sin(angle)
    return (
        cosine * vector[0] - sine * vector[1],
        sine * vector[0] + cosine * vector[1],
    )


def apply_ik_constraints(parts, frame, frame_index, width, height):
    """Solve optional two-bone IK chains into deterministic local rotations."""
    raw_constraints = frame.get("ik", [])
    if not isinstance(raw_constraints, list):
        fail(f"frame {frame_index + 1} ik must be an array")
    part_by_name = {part["name"]: part for part in parts}
    normalized_constraints = []
    constrained_parts = set()
    for constraint_index, raw_constraint in enumerate(raw_constraints):
        label = f"frame {frame_index + 1} ik {constraint_index + 1}"
        if not isinstance(raw_constraint, dict):
            fail(f"{label} must be an object")
        unexpected = set(raw_constraint) - {
            "chain", "endAnchor", "target", "bend", "endRotation",
        }
        if unexpected:
            fail(f"{label} has unsupported keys: {', '.join(sorted(unexpected))}")
        raw_chain = raw_constraint.get("chain")
        if not isinstance(raw_chain, list) or len(raw_chain) not in {2, 3}:
            fail(f"{label} chain must contain upper, lower, and optionally foot parts")
        chain = [slug(name) for name in raw_chain]
        if len(set(chain)) != len(chain) or any(name not in part_by_name for name in chain):
            fail(f"{label} chain contains duplicate or unknown parts")
        upper, lower = (part_by_name[chain[0]], part_by_name[chain[1]])
        shared = constrained_parts.intersection(chain)
        if shared:
            fail(f"{label} reuses already constrained parts: {', '.join(sorted(shared))}")
        constrained_parts.update(chain)
        if lower.get("parent") != upper["name"]:
            fail(f"{label} lower part must be a direct child of its upper part")
        if len(chain) == 3 and part_by_name[chain[2]].get("parent") != lower["name"]:
            fail(f"{label} foot part must be a direct child of its lower part")
        upper_start_name = upper.get("attach", {}).get("selfAnchor")
        if not upper_start_name:
            candidates = [
                name for name in upper["anchors"]
                if any(token in name for token in ("hip", "shoulder", "root", "start"))
            ]
            if len(candidates) != 1:
                fail(
                    f"{label} cannot infer the upper start anchor; attach the upper part "
                    "to its body or name one anchor hip/shoulder/root/start"
                )
            upper_start_name = candidates[0]
        lower_attach = lower.get("attach")
        if not lower_attach:
            fail(f"{label} lower part requires attachment anchors")
        upper_joint_name = lower_attach["parentAnchor"]
        lower_joint_name = lower_attach["selfAnchor"]
        upper_start = upper["anchors"][upper_start_name]
        upper_joint = upper["anchors"][upper_joint_name]
        lower_joint = lower["anchors"][lower_joint_name]
        if point_distance(upper.get("pivot", [0, 0]), upper_start) > 0.25:
            fail(f"{label} upper pivot must coincide with its chain start anchor")
        if point_distance(lower.get("pivot", [0, 0]), lower_joint) > 0.25:
            fail(f"{label} lower pivot must coincide with its attachment anchor")
        end_part = part_by_name[chain[-1]]
        end_anchor_name = slug(raw_constraint.get("endAnchor", ""))
        if end_anchor_name not in end_part["anchors"]:
            fail(f"{label} refers to missing endpoint anchor {end_part['name']}.{end_anchor_name}")
        end_anchor = end_part["anchors"][end_anchor_name]
        if len(chain) == 3:
            foot = end_part
            foot_attach = foot.get("attach")
            if not foot_attach:
                fail(f"{label} foot part requires attachment anchors")
            lower_end_name = foot_attach["parentAnchor"]
            foot_start_name = foot_attach["selfAnchor"]
            lower_end = lower["anchors"][lower_end_name]
            foot_start = foot["anchors"][foot_start_name]
            if point_distance(foot.get("pivot", [0, 0]), foot_start) > 0.25:
                fail(f"{label} foot pivot must coincide with its attachment anchor")
        else:
            lower_end_name = end_anchor_name
            lower_end = lower["anchors"][lower_end_name]
            foot_start = None

        target = raw_constraint.get("target")
        if not isinstance(target, list) or len(target) != 2:
            fail(f"{label} target must be [x,y] in locked-canvas coordinates")
        target_x = finite_number(target[0], f"{label} target x")
        target_y = finite_number(target[1], f"{label} target y")
        if not (0 <= target_x <= width and 0 <= target_y <= height):
            fail(f"{label} target lies outside the locked canvas")
        bend = finite_number(raw_constraint.get("bend", 1), f"{label} bend")
        if bend not in {-1, 1}:
            fail(f"{label} bend must be -1 or 1")
        end_rotation = finite_number(
            raw_constraint.get("endRotation", 0), f"{label} endRotation",
        )

        transforms = frame.setdefault("transforms", {})
        for part_name in chain[:2]:
            transform = transforms.setdefault(part_name, {})
            for key, default in (("dx", 0), ("dy", 0), ("scaleX", 1), ("scaleY", 1)):
                if abs(float(transform.get(key, default)) - default) > 1e-6:
                    fail(f"{label} cannot solve a chain with translated or scaled bone {part_name!r}")

        current_matrices = resolve_frame_matrices(parts, frame)
        parent_name = upper.get("parent")
        if parent_name:
            parent_world = current_matrices[parent_name]
        else:
            root = frame.get("root", {})
            parent_world = translation_matrix(root.get("dx", 0), root.get("dy", 0))
        desired_lower_end_world = (target_x, target_y)
        if len(chain) == 3:
            endpoint_vector = (
                end_anchor[0] - foot_start[0],
                end_anchor[1] - foot_start[1],
            )
            endpoint_vector = rotated_vector(endpoint_vector, end_rotation)
            desired_lower_end_world = (
                target_x - endpoint_vector[0],
                target_y - endpoint_vector[1],
            )
        desired_lower_end = apply_matrix(invert_matrix(parent_world), desired_lower_end_world)

        first_vector = (
            upper_joint[0] - upper_start[0],
            upper_joint[1] - upper_start[1],
        )
        second_vector = (
            lower_end[0] - lower_joint[0],
            lower_end[1] - lower_joint[1],
        )
        first_length = math.hypot(*first_vector)
        second_length = math.hypot(*second_vector)
        target_vector = (
            desired_lower_end[0] - upper_start[0],
            desired_lower_end[1] - upper_start[1],
        )
        target_distance = math.hypot(*target_vector)
        if first_length < 0.5 or second_length < 0.5:
            fail(f"{label} bone anchors must describe two non-zero segment lengths")
        minimum_reach = abs(first_length - second_length)
        maximum_reach = first_length + second_length
        if target_distance < minimum_reach - 1e-5 or target_distance > maximum_reach + 1e-5:
            fail(
                f"{label} target is unreachable at {target_distance:.2f}px; "
                f"the chain reach is {minimum_reach:.2f}–{maximum_reach:.2f}px"
            )
        if target_distance < 1e-6:
            fail(f"{label} target collapses the two-bone chain onto its start anchor")
        relative_cosine = (
            target_distance * target_distance - first_length * first_length - second_length * second_length
        ) / (2 * first_length * second_length)
        relative_angle = bend * math.acos(max(-1.0, min(1.0, relative_cosine)))
        target_angle = math.atan2(target_vector[1], target_vector[0])
        first_angle = target_angle - math.atan2(
            second_length * math.sin(relative_angle),
            first_length + second_length * math.cos(relative_angle),
        )
        bind_first_angle = math.atan2(first_vector[1], first_vector[0])
        bind_second_angle = math.atan2(second_vector[1], second_vector[0])
        upper_rotation = math.degrees(first_angle - bind_first_angle)
        lower_rotation = math.degrees(relative_angle - (bind_second_angle - bind_first_angle))
        transforms[upper["name"]] = {**transforms[upper["name"]], "rotate": upper_rotation}
        transforms[upper["name"]].pop("worldRotate", None)
        transforms[lower["name"]] = {**transforms[lower["name"]], "rotate": lower_rotation}
        transforms[lower["name"]].pop("worldRotate", None)
        if len(chain) == 3:
            foot_transform = transforms.setdefault(end_part["name"], {})
            for key, default in (("dx", 0), ("dy", 0), ("scaleX", 1), ("scaleY", 1)):
                if abs(float(foot_transform.get(key, default)) - default) > 1e-6:
                    fail(f"{label} cannot lock a translated or scaled foot")
            transforms[end_part["name"]] = {
                **foot_transform,
                "worldRotate": end_rotation,
            }
            transforms[end_part["name"]].pop("rotate", None)
        normalized_constraints.append({
            "chain": chain,
            "endAnchor": end_anchor_name,
            "target": [target_x, target_y],
            "bend": int(bend),
            "endRotation": end_rotation,
        })
        solved_matrices = resolve_frame_matrices(parts, frame)
        solved_endpoint = apply_matrix(solved_matrices[end_part["name"]], end_anchor)
        residual = point_distance(solved_endpoint, (target_x, target_y))
        if residual > 0.25:
            fail(f"{label} solver missed its target by {residual:.2f}px")
    frame["ik"] = normalized_constraints


def validate_world_rotation_support(parts, frame, frame_index):
    transforms = frame.get("transforms", {})
    part_by_name = {part["name"]: part for part in parts}
    if "worldRotate" in transforms.get("base", {}):
        fail(f"frame {frame_index + 1} base cannot use worldRotate")
    for part_name, transform in transforms.items():
        if part_name == "base" or "worldRotate" not in transform:
            continue
        current_name = part_name
        while current_name:
            current_transform = transforms.get(current_name, {})
            scale_x = float(current_transform.get("scaleX", 1))
            scale_y = float(current_transform.get("scaleY", 1))
            if scale_x <= 0 or scale_y <= 0 or abs(scale_x - scale_y) > 1e-6:
                fail(
                    f"frame {frame_index + 1} worldRotate on {part_name!r} requires "
                    f"positive uniform scale throughout its ancestor chain; {current_name!r} "
                    f"uses scaleX={scale_x:g}, scaleY={scale_y:g}"
                )
            current_name = part_by_name[current_name].get("parent")


def visible_pixel_count(pixels):
    return sum(1 for offset in range(3, len(pixels), 4) if pixels[offset])


def alpha_component_sizes(pixels, width, height):
    remaining = {
        (x, y)
        for y in range(height)
        for x in range(width)
        if pixels[(y * width + x) * 4 + 3]
    }
    sizes = []
    while remaining:
        stack = [remaining.pop()]
        size = 0
        while stack:
            x, y = stack.pop()
            size += 1
            for offset_y in (-1, 0, 1):
                for offset_x in (-1, 0, 1):
                    neighbor = (x + offset_x, y + offset_y)
                    if neighbor in remaining:
                        remaining.remove(neighbor)
                        stack.append(neighbor)
        sizes.append(size)
    return sorted(sizes, reverse=True)


def prepare_layers(source, width, height, parts, rig_version):
    """Split the locked master into a base layer and named movable layers."""
    base = bytearray(source)
    raw_layers = [layer_from_mask(source, width, height, part["mask"]) for part in parts]
    if rig_version >= 2:
        for y in range(height):
            for x in range(width):
                offset = (y * width + x) * 4
                if not source[offset + 3]:
                    continue
                selectors = [
                    index for index, layer in enumerate(raw_layers)
                    if layer[offset + 3]
                ]
                if not selectors:
                    continue
                joint_cap = any(parts[index].get("overlapMode") == "joint-cap" for index in selectors)
                if not joint_cap:
                    owner = max(
                        selectors,
                        key=lambda index: (int(parts[index].get("z", index)), index),
                    )
                    for index in selectors:
                        if index != owner:
                            raw_layers[index][offset:offset + 4] = b"\x00\x00\x00\x00"
    layers = []
    for index, (part, layer) in enumerate(zip(parts, raw_layers)):
        if not visible_pixel_count(layer):
            fail(f"part {part['name']!r} selected no exclusively owned visible pixels")
        if not part.get("keepInBase", False):
            clear_mask(base, width, height, part["mask"])
        layers.append({
            "name": part["name"],
            "pixels": layer,
            "pivot": part.get("pivot", [width / 2, height / 2]),
            "z": int(part.get("z", index)),
            "mesh": part.get("mesh"),
        })
    return base, layers


def frame_clipped_pixels(pixels, width, height, matrix):
    """Count opaque source cells whose transformed area leaves the canvas.

    Sampling only the pixel center misses the common case where a rotated or
    scaled edge cell is visibly shaved by the canvas boundary.  Treat each
    source pixel as a unit square and require all four transformed corners to
    remain inside the locked canvas.
    """
    clipped = 0
    for y in range(height):
        for x in range(width):
            offset = (y * width + x) * 4
            if not pixels[offset + 3]:
                continue
            corners = (
                apply_matrix(matrix, (x, y)),
                apply_matrix(matrix, (x + 1, y)),
                apply_matrix(matrix, (x, y + 1)),
                apply_matrix(matrix, (x + 1, y + 1)),
            )
            if any(
                not (0 <= destination_x <= width and 0 <= destination_y <= height)
                for destination_x, destination_y in corners
            ):
                clipped += 1
    return clipped


def mesh_clipped_pixels(pixels, width, height, mesh, matrices):
    clipped = 0
    destination_vertices = mesh_destination_vertices(mesh, matrices)
    for y in range(height):
        for x in range(width):
            offset = (y * width + x) * 4
            if not pixels[offset + 3]:
                continue
            destination = mesh_destination_point(
                mesh, destination_vertices, (x + 0.5, y + 0.5),
            )
            if destination is None or not (0 <= destination[0] < width and 0 <= destination[1] < height):
                clipped += 1
    return clipped


def render_frame(width, height, base, layers, parts, frame, palette, base_z=0, rig_version=2):
    matrices = resolve_frame_matrices(parts, frame)
    canvas = Canvas(width, height)
    if rig_version == 1:
        composite(canvas.data, transformed_matrix(base, width, height, matrices["base"]))
        for command in frame.get("underlay", []):
            canvas.command(command, palette)
        for layer in sorted(layers, key=lambda item: item["z"]):
            composite(canvas.data, transformed_matrix(
                layer["pixels"], width, height, matrices[layer["name"]],
            ))
        for command in frame.get("overlay", []):
            canvas.command(command, palette)
        return canvas, matrices
    for command in frame.get("underlay", []):
        canvas.command(command, palette)
    z_overrides = frame.get("zOverrides", {})
    draw_items = [{
        "name": "base",
        "pixels": base,
        "z": int(z_overrides.get("base", base_z)),
        "order": -1,
    }]
    for order, layer in enumerate(layers):
        draw_items.append({
            "name": layer["name"],
            "pixels": layer["pixels"],
            "z": int(z_overrides.get(layer["name"], layer["z"])),
            "order": order,
        })
    for item in sorted(draw_items, key=lambda value: (value["z"], value["order"])):
        layer = next((value for value in layers if value["name"] == item["name"]), None)
        if layer is not None and layer.get("mesh"):
            rendered = transformed_mesh(
                item["pixels"], width, height, layer["mesh"], matrices,
            )
        else:
            rendered = transformed_matrix(
                item["pixels"], width, height, matrices[item["name"]],
            )
        composite(canvas.data, rendered)
    for command in frame.get("overlay", []):
        canvas.command(command, palette)
    return canvas, matrices


def point_distance(first, second):
    return math.hypot(first[0] - second[0], first[1] - second[1])


def frame_signature(parts, matrices):
    signature = []
    for part in parts:
        matrix = matrices[part["name"]]
        points = [part.get("pivot", [0, 0]), *part.get("anchors", {}).values()]
        signature.extend(apply_matrix(matrix, point) for point in points)
    return signature


def signature_energy(first, second):
    if len(first) != len(second) or not first:
        return 0.0
    return sum(point_distance(a, b) for a, b in zip(first, second)) / len(first)


def validate_articulated_motion(spec, names, frames, parts=None, matrices_by_frame=None):
    """Reject locomotion rigs that merely translate one rigid sprite."""
    morphology, motion_text, is_locomotion, _ = locomotion_context(spec)
    if not is_locomotion:
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
    meaningful_parts = meaningfully_animated_parts(frames)
    if len(meaningful_parts) < 2:
        fail(
            "locomotion transforms are too small to survive sprite-pixel quantization: "
            "animate at least two anatomical parts across 8 degrees, 1.5 pixels, or 5% scale "
            "instead of publishing a nearly static root bob"
        )

    if int(spec.get("rigVersion", 1)) < 2:
        return
    if morphology not in {
        "biped", "quadruped", "hexapod", "segmented-many-leg", "serpentine", "winged"
    }:
        fail("rigVersion 2 locomotion requires a supported proposal.morphologyTag")
    if not parts or not matrices_by_frame:
        fail("rigVersion 2 locomotion could not resolve its articulated skeleton")
    descriptors = {
        part["name"]: f"{part['name']} {part.get('role', '')}".replace("-", "_").lower()
        for part in parts
    }
    core_tokens = {
        "biped": {"body", "torso", "pelvis", "spine", "chest", "hip"},
        "quadruped": {"body", "torso", "pelvis", "spine", "chest"},
        "hexapod": {"body", "thorax", "abdomen", "head"},
        "segmented-many-leg": {"body", "segment", "segments", "head", "tail"},
        "serpentine": {"body", "segment", "segments", "head", "tail", "spine"},
        "winged": {"body", "torso", "chest", "spine"},
    }
    required_core_parts = 2 if morphology in {"segmented-many-leg", "serpentine"} else 1
    core_parts = {
        name for name, descriptor in descriptors.items()
        if exact_tokens(descriptor).intersection(core_tokens.get(morphology, set()))
    }
    animated_core = meaningful_parts.intersection(core_parts)
    if core_parts and len(animated_core) < required_core_parts:
        fail(
            f"{morphology} locomotion ignores the body: animate at least "
            f"{required_core_parts} core body part{'s' if required_core_parts != 1 else ''} "
            "with visible compression, rotation, spine wave, or counter-motion; moving only "
            "limbs under a rigid body is not a finished rig"
        )
    if morphology == "biped":
        left_leg_parts = {
            name for name, descriptor in descriptors.items()
            if "left" in exact_tokens(descriptor)
            and exact_tokens(descriptor).intersection({"leg", "foot", "shin", "thigh"})
        }
        right_leg_parts = {
            name for name, descriptor in descriptors.items()
            if "right" in exact_tokens(descriptor)
            and exact_tokens(descriptor).intersection({"leg", "foot", "shin", "thigh"})
        }
        if len(left_leg_parts) < 2 or len(right_leg_parts) < 2:
            fail(
                "a rigVersion 2 biped walk requires articulated left and right leg chains "
                "with at least two parts per side"
            )
        moving_world_parts = set()
        for part in parts:
            points = [part.get("pivot", [0, 0]), *part.get("anchors", {}).values()]
            positions = [
                tuple(apply_matrix(matrices[part["name"]], point) for point in points)
                for matrices in matrices_by_frame
            ]
            if any(positions[index] != positions[0] for index in range(1, len(positions))):
                moving_world_parts.add(part["name"])
        if not (moving_world_parts & left_leg_parts) or not (moving_world_parts & right_leg_parts):
            fail("a biped locomotion cycle must visibly articulate both left and right leg chains")

    motion_tokens = exact_tokens(motion_text.replace(" ", "_"))
    if morphology == "quadruped" and motion_tokens.intersection(
        {"hop", "hopping", "bound", "bounding", "pounce"}
    ):
        normalized = {name.replace("-", "_").lower() for name in names}
        has_hind = any(any(token in name for token in ("hind", "rear", "haunch", "thigh")) for name in normalized)
        has_fore = any(any(token in name for token in ("fore", "front", "shoulder")) for name in normalized)
        if not has_hind or not has_fore or len(moved_parts) < 3:
            fail(
                "a quadruped hop requires independently animated hindlimb and forelimb masks "
                "plus at least one additional articulated body part; whole-body lift is not a hop"
            )


def part_side(part):
    descriptor = exact_tokens(f"{part['name']}_{part.get('role', '')}")
    if "left" in descriptor:
        return "left"
    if "right" in descriptor:
        return "right"
    if descriptor.intersection({"fore", "front"}):
        return "front"
    if descriptor.intersection({"hind", "rear"}):
        return "hind"
    return part["name"]


def visible_bounds(pixels, width, height):
    """Return the inclusive alpha bounds without depending on palette colors."""
    xs = []
    ys = []
    for y in range(height):
        for x in range(width):
            if pixels[(y * width + x) * 4 + 3]:
                xs.append(x)
                ys.append(y)
    if not xs:
        return None
    return min(xs), min(ys), max(xs), max(ys)


def endpoint_anchor(part):
    """Choose the gameplay contact/end anchor for a hand, foot, or paw part."""
    preferred = ("toe", "sole", "paw", "hoof", "foot", "ground", "tip", "end")
    anchors = part.get("anchors", {})
    for token in preferred:
        for name, point in anchors.items():
            if token in exact_tokens(name):
                return point
    return next(reversed(list(anchors.values())), part.get("pivot", [0, 0]))


def part_rotation_span(part, frames):
    values = [
        float(frame.get("transforms", {}).get(part["name"], {}).get("rotate", 0))
        for frame in frames
    ]
    return max(values, default=0.0) - min(values, default=0.0)


def nearest_visible_distance(source, width, height, mask, point):
    nearest = float("inf")
    for y in range(height):
        for x in range(width):
            offset = (y * width + x) * 4
            if source[offset + 3] and mask_contains(mask, x, y):
                nearest = min(nearest, point_distance((x + 0.5, y + 0.5), point))
    return nearest


def validate_v2_motion_quality(
    spec, width, height, source, parts, frames, base, layers, matrices_by_frame, canvases,
):
    """Hard-gate mechanical failures that whole-frame image metrics cannot see."""
    morphology, _, is_locomotion, is_grounded_walk = locomotion_context(spec)
    looping = spec.get("looping", True)
    if not isinstance(looping, bool):
        fail("looping must be true or false")
    root_motion = str(spec.get("rootMotion", "")).strip().lower()
    if root_motion not in {"in-place", "baked"}:
        fail("rigVersion 2 requires rootMotion to be 'in-place' or 'baked'")
    if is_locomotion and not all(str(frame.get("phase", "")).strip() for frame in frames):
        fail("rigVersion 2 locomotion requires a non-empty phase on every frame")
    phases = [str(frame.get("phase", "")).strip().lower() for frame in frames]
    if is_locomotion and len(set(phases)) != len(phases):
        fail("locomotion phase names must be unique within one cycle")

    roots = [frame.get("root", {}) for frame in frames]
    root_x = [float(root.get("dx", 0)) for root in roots]
    root_y = [float(root.get("dy", 0)) for root in roots]
    if is_locomotion and root_motion == "in-place":
        root_amplitude_limit = max(1.0, min(width, height) / 64.0)
        root_span_limit = root_amplitude_limit * 2
        if max(root_x) - min(root_x) > root_span_limit + 1e-6:
            fail(
                f"in-place root drift spans {max(root_x) - min(root_x):.2f}px; "
                f"the limit is {root_span_limit:.2f}px"
            )
        if looping and abs(root_x[-1] - root_x[0]) > root_amplitude_limit + 1e-6:
            fail("in-place root motion snaps horizontally across the loop seam")
    vertical_limit = max(2.0, min(width, height) / 24.0)
    if is_locomotion and max(root_y) - min(root_y) > vertical_limit + 1e-6:
        fail("root bob is too large for a stable locomotion cycle")

    part_by_name = {part["name"]: part for part in parts}
    if is_locomotion and morphology == "quadruped":
        limb_groups = set()
        limb_group_counts = {}
        locomotor_parts = []
        for part in parts:
            descriptor = exact_tokens(f"{part['name']}_{part.get('role', '')}")
            axis = "hind" if descriptor.intersection({"hind", "rear"}) else (
                "fore" if descriptor.intersection({"fore", "front"}) else None
            )
            side = "near" if "near" in descriptor else ("far" if "far" in descriptor else None)
            if axis and descriptor.intersection({"leg", "limb", "paw", "hoof"}):
                locomotor_parts.append(part)
                if side:
                    limb_groups.add((axis, side))
                    limb_group_counts[(axis, side)] = limb_group_counts.get((axis, side), 0) + 1
        required_groups = {("hind", "near"), ("hind", "far"), ("fore", "near"), ("fore", "far")}
        if not required_groups.issubset(limb_groups):
            missing = ", ".join("_".join(group) for group in sorted(required_groups - limb_groups))
            fail(f"quadruped locomotion is missing independently described limb groups: {missing}")
        if int(spec.get("rigVersion", 1)) >= 3:
            underspecified = [
                group for group in sorted(required_groups)
                if limb_group_counts.get(group, 0) < 3
            ]
            if underspecified:
                names = ", ".join("_".join(group) for group in underspecified)
                fail(
                    f"four_leg_sprite_rig requires upper/lower/end anatomy for: {names}; "
                    "a weighted silhouette cannot replace real joints"
                )
        else:
            for part in locomotor_parts:
                child_is_limb = any(
                    candidate.get("parent") == part["name"]
                    and exact_tokens(candidate.get("role", "")).intersection({"leg", "limb", "paw", "hoof"})
                    for candidate in parts
                )
                if not part.get("mesh") and not child_is_limb:
                    fail(
                        f"quadruped limb {part['name']!r} is one rigid cutout; split it into an "
                        "articulated chain or give it a weighted mesh"
                    )
        motion_tokens = exact_tokens(spec.get("proposal", {}).get("motionIntent", ""))
        if motion_tokens.intersection({"run", "running", "gallop", "sprint", "bound", "bounding"}):
            suspension_count = sum(not frame.get("contacts") for frame in frames)
            if suspension_count < 2:
                fail(
                    "a quadruped run or gallop requires both extended and gathered "
                    "suspension phases"
                )
            contact_axes = set()
            for frame in frames:
                for contact in frame.get("contacts", []):
                    descriptor = exact_tokens(
                        f"{contact['part']}_{part_by_name[contact['part']].get('role', '')}"
                    )
                    if descriptor.intersection({"hind", "rear"}):
                        contact_axes.add("hind")
                    if descriptor.intersection({"fore", "front"}):
                        contact_axes.add("fore")
            if contact_axes != {"hind", "fore"}:
                fail("a quadruped run or gallop must show both hind-drive and fore-impact contacts")

            upper_by_group = {}
            paw_parts = []
            for part in locomotor_parts:
                descriptor = exact_tokens(f"{part['name']}_{part.get('role', '')}")
                axis = "hind" if descriptor.intersection({"hind", "rear"}) else (
                    "fore" if descriptor.intersection({"fore", "front"}) else None
                )
                side = "near" if "near" in descriptor else ("far" if "far" in descriptor else None)
                if axis and side and descriptor.intersection({"upper", "shoulder", "thigh", "haunch"}):
                    upper_by_group[(axis, side)] = part
                if descriptor.intersection({"paw", "hoof", "foot"}):
                    paw_parts.append(part)
            shallow_groups = [
                (group, part_rotation_span(part, frames) if part else 0.0)
                for group in sorted(required_groups)
                for part in [upper_by_group.get(group)]
                if part is None or part_rotation_span(part, frames) < 22.0
            ]
            if shallow_groups:
                detail = ", ".join(
                    f"{'_'.join(group)}={span:.1f}°" for group, span in shallow_groups
                )
                fail(
                    "quadruped run limb strokes are too shallow to read as running "
                    f"({detail}); each fore/hind upper limb needs at least a 22° cycle span"
                )

            pose_tokens = [exact_tokens(frame.get("pose", "")) for frame in frames]
            extended_indices = [
                index for index, tokens in enumerate(pose_tokens)
                if "extended" in tokens and tokens.intersection({"flight", "suspension"})
            ]
            gathered_indices = [
                index for index, tokens in enumerate(pose_tokens)
                if "gathered" in tokens and tokens.intersection({"flight", "suspension", "recovery"})
            ]
            if not extended_indices or not gathered_indices:
                fail("a quadruped run requires distinct extended_flight and gathered_flight poses")
            if len(paw_parts) >= 4:
                def paw_spread(frame_index):
                    matrices = matrices_by_frame[frame_index]
                    xs = [
                        apply_matrix(matrices[part["name"]], endpoint_anchor(part))[0]
                        for part in paw_parts
                    ]
                    return max(xs) - min(xs)

                extended_spread = max(paw_spread(index) for index in extended_indices)
                gathered_spread = min(paw_spread(index) for index in gathered_indices)
                bounds = visible_bounds(source, width, height)
                source_width = bounds[2] - bounds[0] + 1 if bounds else width
                required_difference = max(3.0, source_width * 0.08)
                if extended_spread - gathered_spread < required_difference:
                    fail(
                        "quadruped flight poses keep the same standing silhouette: "
                        f"extended paw spread {extended_spread:.2f}px versus gathered "
                        f"{gathered_spread:.2f}px; increase extension/compression by at least "
                        f"{required_difference:.2f}px"
                    )
    if morphology == "winged":
        wing_roots = []
        for part in parts:
            descriptor = exact_tokens(f"{part['name']}_{part.get('role', '')}")
            if "wing" not in descriptor or not descriptor.intersection({"root", "upper", "shoulder"}):
                continue
            wing_roots.append(part)
            parent_name = part.get("parent")
            if not parent_name:
                fail(f"wing root {part['name']!r} must attach to a torso, chest, or body part")
            parent = part_by_name[parent_name]
            parent_descriptor = exact_tokens(f"{parent['name']}_{parent.get('role', '')}")
            if "wing" in parent_descriptor or not parent_descriptor.intersection(
                {"body", "torso", "chest", "spine", "shoulder"}
            ):
                fail(
                    f"wing root {part['name']!r} is parented to {parent_name!r}; "
                    "each wing root must attach independently to the torso/chest, never to the other wing"
                )
            rotations = [
                float(frame.get("transforms", {}).get(part["name"], {}).get("rotate", 0))
                for frame in frames
            ]
            rotation_span = max(rotations) - min(rotations)
            if rotation_span > 30.0 + 1e-6:
                fail(
                    f"wing root {part['name']!r} rotates across {rotation_span:.2f} degrees; "
                    "keep the root seated within a 30-degree stroke and put additional folding in the membrane"
                )
            translations = [
                math.hypot(
                    float(frame.get("transforms", {}).get(part["name"], {}).get("dx", 0)),
                    float(frame.get("transforms", {}).get(part["name"], {}).get("dy", 0)),
                )
                for frame in frames
            ]
            if max(translations, default=0.0) > 1.0 + 1e-6:
                fail(
                    f"wing root {part['name']!r} translates away from its shoulder; "
                    "animate wing rotation and membrane folding around the fixed attachment instead"
                )
        if len(wing_roots) < 2:
            fail("winged_sprite_rig requires two independently attached wing roots")
    for part in parts:
        for anchor_name, anchor in part.get("anchors", {}).items():
            distance = nearest_visible_distance(source, width, height, part["mask"], anchor)
            if distance > 2.25:
                fail(
                    f"part {part['name']!r} anchor {anchor_name!r} is {distance:.2f}px "
                    "from its visible pixels"
                )
        parent_name = part.get("parent")
        if not parent_name:
            continue
        parent = part_by_name[parent_name]
        attach = part["attach"]
        parent_anchor = parent["anchors"][attach["parentAnchor"]]
        self_anchor = part["anchors"][attach["selfAnchor"]]
        bind_gap = point_distance(parent_anchor, self_anchor)
        if bind_gap > 1.0:
            fail(
                f"joint {parent_name}->{part['name']} bind anchors are {bind_gap:.2f}px apart; "
                "attachment anchors must coincide"
            )
        for frame_index, matrices in enumerate(matrices_by_frame):
            parent_world = apply_matrix(matrices[parent_name], parent_anchor)
            child_world = apply_matrix(matrices[part["name"]], self_anchor)
            gap = point_distance(parent_world, child_world)
            if gap > 1.25:
                fail(
                    f"joint {parent_name}->{part['name']} separates by {gap:.2f}px "
                    f"in frame {frame_index + 1}"
                )

    if is_grounded_walk and morphology == "biped":
        for frame_index, frame in enumerate(frames):
            for part in parts:
                role_tokens = exact_tokens(part.get("role", ""))
                transform = frame.get("transforms", {}).get(part["name"], {})
                if "leg" in role_tokens and "upper" in role_tokens:
                    limit = 60.0
                    value = abs(float(transform.get("rotate", 0)))
                elif "leg" in role_tokens and "lower" in role_tokens:
                    limit = 75.0
                    value = abs(float(transform.get("rotate", 0)))
                elif "foot" in role_tokens:
                    limit = 30.0
                    value = abs(float(transform.get("worldRotate", transform.get("rotate", 0))))
                else:
                    continue
                if value > limit + 1e-6:
                    fail(
                        f"frame {frame_index + 1} {part['name']!r} bends {value:.2f} degrees; "
                        f"the biped walk limit for role {part['role']!r} is {limit:.2f} degrees"
                    )

    contacts_by_anchor = {}
    support_sides = []
    for frame_index, (frame, matrices) in enumerate(zip(frames, matrices_by_frame)):
        contacts = frame.get("contacts", [])
        if is_grounded_walk and not contacts:
            fail(f"grounded locomotion frame {frame_index + 1} has no planted support contact")
        sides = set()
        for contact in contacts:
            part = part_by_name[contact["part"]]
            if is_grounded_walk and morphology == "biped":
                descriptor = exact_tokens(f"{part['name']}_{part.get('role', '')}")
                anchor_descriptor = exact_tokens(contact["anchor"])
                is_lower_leg = "lower" in descriptor and "leg" in descriptor
                if not (is_lower_leg or descriptor.intersection({"foot", "shin", "paw", "hoof"})):
                    fail(
                        f"biped contact {part['name']}.{contact['anchor']} is not on a "
                        "lower-leg or foot support part"
                    )
                if not anchor_descriptor.intersection({"foot", "sole", "toe", "paw", "hoof", "ground"}):
                    fail(
                        f"biped contact {part['name']}.{contact['anchor']} is not a "
                        "ground-contact anchor such as sole, toe, or foot"
                    )
            anchor = part["anchors"][contact["anchor"]]
            world = apply_matrix(matrices[part["name"]], anchor)
            key = (part["name"], contact["anchor"])
            contacts_by_anchor.setdefault(key, []).append((frame_index, world))
            sides.add(part_side(part))
        support_sides.append(sides)
    contact_limit = max(1.0, min(width, height) / 64.0)
    for (part_name, anchor_name), samples in contacts_by_anchor.items():
        runs = []
        for sample in sorted(samples):
            if not runs or sample[0] != runs[-1][-1][0] + 1:
                runs.append([sample])
            else:
                runs[-1].append(sample)
        if looping and len(runs) > 1 and runs[0][0][0] == 0 and runs[-1][-1][0] == len(frames) - 1:
            runs[0] = runs[-1] + runs[0]
            runs.pop()
        for run in runs:
            positions = [sample[1] for sample in run]
            span = max(
                (point_distance(first, second) for first in positions for second in positions),
                default=0.0,
            )
            if span > contact_limit + 1e-6:
                frames_text = ", ".join(str(sample[0] + 1) for sample in run)
                fail(
                    f"planted contact {part_name}.{anchor_name} slides {span:.2f}px "
                    f"across frames {frames_text}; the limit is {contact_limit:.2f}px"
                )
    if is_grounded_walk and morphology == "biped":
        observed = set().union(*support_sides) if support_sides else set()
        if not {"left", "right"}.issubset(observed):
            fail("a biped walk must plant both left and right support groups during the cycle")
        exchanges = 0
        pairs = list(zip(support_sides, support_sides[1:]))
        if looping and support_sides:
            pairs.append((support_sides[-1], support_sides[0]))
        for current, following in pairs:
            if current != following and (
                ("left" in current and "right" in following)
                or ("right" in current and "left" in following)
            ):
                exchanges += 1
        minimum_exchanges = 2 if looping else 1
        if exchanges < minimum_exchanges:
            fail(
                "a biped walk must alternate support "
                f"at least {minimum_exchanges} time(s) across the sequence"
            )

    motion_tokens = exact_tokens(spec.get("proposal", {}).get("motionIntent", ""))
    if is_locomotion and morphology == "biped" and motion_tokens.intersection(
        {"run", "running", "sprint"}
    ):
        if sum(not frame.get("contacts") for frame in frames) < 2:
            fail("a biped run requires two flight phases, one after each leg drives")
        observed = set().union(*support_sides) if support_sides else set()
        if not {"left", "right"}.issubset(observed):
            fail("a biped run must include distinct left and right contact frames")
        foot_by_side = {}
        for part in parts:
            descriptor = exact_tokens(f"{part['name']}_{part.get('role', '')}")
            side = "left" if "left" in descriptor else ("right" if "right" in descriptor else None)
            if side and descriptor.intersection({"foot", "paw", "hoof"}):
                foot_by_side[side] = part
        if set(foot_by_side) != {"left", "right"}:
            fail("a biped run requires independently anchored left and right feet")
        bounds = visible_bounds(source, width, height)
        source_height = bounds[3] - bounds[1] + 1 if bounds else height
        minimum_split = max(4.0, source_height * 0.20)
        wide_contact_sides = set()
        wide_contact_indices = []
        for index, matrices in enumerate(matrices_by_frame):
            frame_sides = support_sides[index]
            if not frame_sides:
                continue
            left = apply_matrix(
                matrices[foot_by_side["left"]["name"]], endpoint_anchor(foot_by_side["left"]),
            )
            right = apply_matrix(
                matrices[foot_by_side["right"]["name"]], endpoint_anchor(foot_by_side["right"]),
            )
            split = abs(left[0] - right[0])
            if split >= minimum_split:
                wide_contact_sides.update(frame_sides.intersection({"left", "right"}))
                wide_contact_indices.append(index)
        if wide_contact_sides != {"left", "right"}:
            fail(
                "biped run contact poses are visually near-static: require a wide split stance "
                f"of at least {minimum_split:.2f}px for both left and right contacts"
            )
        opposite_extremes = any(
            min((second - first) % len(frames), (first - second) % len(frames))
            >= max(2, len(frames) // 3)
            for first in wide_contact_indices
            for second in wide_contact_indices
            if first != second
        )
        if not opposite_extremes:
            fail("biped run needs two wide split contact extremes spaced across the cycle")

    clipped_by_name = []
    layer_by_name = {layer["name"]: layer for layer in layers}
    for frame_index, matrices in enumerate(matrices_by_frame):
        clipped = frame_clipped_pixels(base, width, height, matrices["base"])
        for part in parts:
            layer = layer_by_name[part["name"]]
            if layer.get("mesh"):
                clipped += mesh_clipped_pixels(
                    layer["pixels"], width, height, layer["mesh"], matrices,
                )
            else:
                clipped += frame_clipped_pixels(
                    layer["pixels"], width, height, matrices[part["name"]],
                )
        owned_visible_pixels = visible_pixel_count(base) + sum(
            visible_pixel_count(layer["pixels"]) for layer in layers
        )
        clipping_limit = max(1, int(math.floor(owned_visible_pixels * 0.01)))
        if clipped > clipping_limit:
            clipped_by_name.append((frame_index + 1, clipped))
    if clipped_by_name:
        frame_index, clipped = clipped_by_name[0]
        fail(
            f"frame {frame_index} clips {clipped} transformed source pixels at the canvas edge; "
            f"the tolerance is {clipping_limit}"
        )

    if is_locomotion:
        source_components = alpha_component_sizes(source, width, height)
        source_component_count = len(source_components)
        for frame_index, canvas in enumerate(canvases):
            rendered_components = alpha_component_sizes(canvas.data, width, height)
            if len(rendered_components) > source_component_count:
                extras = rendered_components[source_component_count:]
                fail(
                    f"frame {frame_index + 1} creates detached alpha fragments "
                    f"with sizes {extras}; moving masks left source pixels behind or a joint opened"
                )

    rendered_hashes = [hashlib.sha256(bytes(canvas.data)).hexdigest() for canvas in canvases]
    if looping and rendered_hashes[-1] == rendered_hashes[0]:
        fail("duplicate loop endpoint: the final rendered frame repeats the first frame")
    for index in range(1, len(rendered_hashes)):
        if rendered_hashes[index] == rendered_hashes[index - 1] and not frames[index].get("hold", False):
            fail(
                f"rendered frames {index} and {index + 1} are identical; "
                "mark an intentional internal hold or create a distinct pose"
            )
    first_seen_hash = {}
    for index, rendered_hash in enumerate(rendered_hashes):
        previous = first_seen_hash.get(rendered_hash)
        if previous is not None:
            is_adjacent_hold = index == previous + 1 and frames[index].get("hold", False)
            if not is_adjacent_hold:
                fail(
                    f"rendered frame {index + 1} repeats non-adjacent frame {previous + 1}; "
                    "every repeated pose must be an adjacent intentional hold"
                )
        else:
            first_seen_hash[rendered_hash] = index

    if not is_locomotion:
        return

    signatures = [frame_signature(parts, matrices) for matrices in matrices_by_frame]
    ordinary = []
    analyzed_ordinary = []
    hold_count = 0
    for index in range(1, len(signatures)):
        energy = signature_energy(signatures[index - 1], signatures[index])
        ordinary.append(energy)
        if frames[index].get("hold", False):
            hold_count += 1
        elif energy > 1e-6:
            analyzed_ordinary.append(energy)
    if hold_count > max(2, len(frames) // 4):
        fail("locomotion contains too many held poses for one readable cycle")
    if not analyzed_ordinary:
        fail("locomotion has no measurable articulated transition energy")
    median_step = statistics.median(analyzed_ordinary)
    scale_unit = max(1.0, min(width, height) / 64.0)
    spike_limit = max(3.5 * scale_unit, median_step * 2.75)
    for index, energy in enumerate(ordinary, start=1):
        if frames[index].get("hold", False):
            continue
        if energy < median_step * 0.25:
            fail(
                f"locomotion nearly pauses between frames {index} and {index + 1}: "
                f"{energy:.2f}px is below 25% of the ordinary transition cadence"
            )
        if energy > spike_limit:
            fail(
                f"limb motion pops between frames {index} and {index + 1}: "
                f"{energy:.2f}px exceeds the {spike_limit:.2f}px transition limit"
            )
    if looping:
        seam_energy = signature_energy(signatures[-1], signatures[0])
        seam_limit = max(2.0 * scale_unit, median_step * 1.75)
        if seam_energy > seam_limit:
            fail(
                f"loop discontinuity is {seam_energy:.2f}px at the final-to-first seam; "
                f"the limit is {seam_limit:.2f}px"
            )
        if is_locomotion and seam_energy < median_step * 0.25:
            fail("loop seam is a near-duplicate pose that will create a visible playback pause")
    elif root_motion == "baked" and is_locomotion:
        deltas = [root_x[index] - root_x[index - 1] for index in range(1, len(root_x))]
        nonzero_signs = {1 if delta > 0 else -1 for delta in deltas if abs(delta) > 1e-6}
        if len(nonzero_signs) > 1:
            fail("non-looping baked root travel must progress monotonically")


def validate_rig(spec, source_path, width, height, source, decoded_master_hash):
    raw_version = spec.get("rigVersion", 1)
    if isinstance(raw_version, bool) or not isinstance(raw_version, int) or raw_version not in {1, 2, 3}:
        fail("rigVersion must be 1, 2, or 3")
    rig_version = raw_version
    if "looping" in spec and not isinstance(spec["looping"], bool):
        fail("looping must be true or false")
    fps = spec.get("fps", 8)
    if isinstance(fps, bool) or not isinstance(fps, int) or not (1 <= fps <= 60):
        fail("fps must be an integer between 1 and 60")
    spec["fps"] = fps
    palette = validate_palette(spec.get("palette", {}))
    spec["palette"] = palette
    frames = spec.get("frames", [])
    if not isinstance(frames, list) or not (2 <= len(frames) <= 32):
        fail("a rig animation must contain between 2 and 32 frames")
    parts = spec.get("parts", [])
    if not isinstance(parts, list) or not parts:
        fail("a rig animation requires at least one movable part")
    if any(not isinstance(part, dict) for part in parts):
        fail("every rig part must be an object")
    raw_names = [str(part.get("name", "")).strip() for part in parts]
    if any(not value for value in raw_names):
        fail("rig part names must be unique and non-empty")
    names = [slug(value) for value in raw_names]
    if len(set(names)) != len(names):
        fail("rig part names must be unique and non-empty")
    for part, name in zip(parts, names):
        part["name"] = name

    base_z = spec.get("baseZ", 0)
    base_z_number = finite_number(base_z, "baseZ")
    if not base_z_number.is_integer():
        fail("baseZ must be an integer")
    spec["baseZ"] = int(base_z_number)

    visible_by_part = {}
    selected_by_pixel = {}
    warnings = []
    for index, part in enumerate(parts):
        label = f"part {names[index]!r}"
        if rig_version == 1 and any(
            key in part for key in ("parent", "role", "anchors", "attach", "overlapMode", "mesh")
        ):
            fail(f"{label} uses hierarchical fields but rigVersion is not 2")
        if rig_version >= 2:
            role = str(part.get("role", "")).strip()
            if not role:
                fail(f"{label} requires a non-empty semantic role in rigVersion 2")
            part["role"] = slug(role)
            if part.get("keepInBase", False):
                fail(f"{label} cannot keep moving pixels in the base in rigVersion 2")
            if "allowOverlap" in part:
                fail(f"{label} must use overlapMode 'joint-cap' instead of legacy allowOverlap")
            overlap_mode = part.get("overlapMode")
            if overlap_mode not in {None, "joint-cap"}:
                fail(f"{label} overlapMode must be 'joint-cap' when present")
            normalize_anchors(part, width, height, label)
            if rig_version >= 3:
                normalize_bone(part, label)
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
        part["pivot"] = [pivot_x, pivot_y]
        z_value = finite_number(part.get("z", index), f"{label} z")
        if not z_value.is_integer():
            fail(f"{label} z must be an integer")
        part["z"] = int(z_value)
        visible = []
        for y in range(height):
            for x in range(width):
                offset = (y * width + x) * 4
                if source[offset + 3] and mask_contains(mask, x, y):
                    visible.append((x, y))
        if not visible:
            fail(f"{label} selected no visible pixels")
        if "mesh" in part:
            if rig_version < 2:
                fail(f"{label} mesh deformation requires rigVersion 2")
            validate_mesh(part["mesh"], width, height, set(names), label)
            uncovered = [
                (x, y) for x, y in visible
                if not mesh_contains_point(part["mesh"], (x + 0.5, y + 0.5))
            ]
            if uncovered:
                sample_x, sample_y = uncovered[0]
                fail(
                    f"{label} mesh does not cover {len(uncovered)} owned visible pixels; "
                    f"first uncovered pixel is ({sample_x},{sample_y})"
                )
        for pixel in visible:
            selected_by_pixel.setdefault(pixel, []).append(names[index])
        visible_by_part[names[index]] = set(visible)

    part_by_name = {part["name"]: part for part in parts}
    if rig_version >= 2:
        for part in parts:
            label = f"part {part['name']!r}"
            raw_parent = part.get("parent")
            if raw_parent is None:
                part["parent"] = None
                if part.get("attach") is not None:
                    fail(f"{label} has attach metadata but no parent")
                continue
            parent_name = slug(raw_parent)
            if parent_name == part["name"]:
                fail(f"{label} cannot parent itself")
            parent = part_by_name.get(parent_name)
            if parent is None:
                fail(f"{label} refers to unknown parent {parent_name!r}")
            part["parent"] = parent_name
            attach = part.get("attach")
            if not isinstance(attach, dict):
                fail(f"{label} requires attach with parentAnchor and selfAnchor")
            unexpected = set(attach) - {"parentAnchor", "selfAnchor"}
            if unexpected or set(attach) != {"parentAnchor", "selfAnchor"}:
                fail(f"{label} attach requires only parentAnchor and selfAnchor")
            parent_anchor = slug(attach.get("parentAnchor", ""))
            self_anchor = slug(attach.get("selfAnchor", ""))
            if parent_anchor not in parent["anchors"]:
                fail(f"{label} attach refers to missing parent anchor {parent_name}.{parent_anchor}")
            if self_anchor not in part["anchors"]:
                fail(f"{label} attach refers to missing self anchor {self_anchor!r}")
            part["attach"] = {"parentAnchor": parent_anchor, "selfAnchor": self_anchor}

        if rig_version >= 3:
            validate_rig_profile(spec, parts, frames, width, height, source)

        overlap_pairs = {}
        for pixel, selectors in selected_by_pixel.items():
            for left_index, left_name in enumerate(selectors):
                for right_name in selectors[left_index + 1:]:
                    pair = tuple(sorted((left_name, right_name)))
                    overlap_pairs.setdefault(pair, set()).add(pixel)
        for (left_name, right_name), shared_pixels in overlap_pairs.items():
            pixel_count = len(shared_pixels)
            left = part_by_name[left_name]
            right = part_by_name[right_name]
            if right.get("parent") == left_name:
                child, parent = right, left
            elif left.get("parent") == right_name:
                child, parent = left, right
            else:
                fail(
                    f"parts {left_name!r} and {right_name!r} overlap {pixel_count} visible pixels "
                    "but are not a declared parent-child joint"
                )
            if child.get("overlapMode") != "joint-cap":
                fail(
                    f"joint {parent['name']}->{child['name']} overlaps {pixel_count} visible pixels; "
                    "declare overlapMode 'joint-cap' or make the masks exclusive"
                )
            smaller_part = min(len(visible_by_part[left_name]), len(visible_by_part[right_name]))
            scale_unit = max(1.0, min(width, height) / 64.0)
            cap_limit = max(1, min(
                int(math.ceil(12 * scale_unit * scale_unit)),
                int(math.ceil(smaller_part * 0.25)),
            ))
            if pixel_count > cap_limit:
                fail(
                    f"joint-cap {parent['name']}->{child['name']} covers {pixel_count} pixels; "
                    f"the bounded cap limit is {cap_limit}"
                )
            attach = child["attach"]
            joint_point = parent["anchors"][attach["parentAnchor"]]
            joint_radius = max(2.5, min(width, height) / 32.0)
            distant = [
                pixel for pixel in shared_pixels
                if point_distance((pixel[0] + 0.5, pixel[1] + 0.5), joint_point) > joint_radius
            ]
            if distant:
                fail(
                    f"joint-cap {parent['name']}->{child['name']} contains overlap pixels "
                    f"outside the {joint_radius:.2f}px attachment neighborhood"
                )
    else:
        claimed = {}
        for part in parts:
            overlap = sorted({claimed[pixel] for pixel in visible_by_part[part["name"]] if pixel in claimed})
            if overlap and not part.get("allowOverlap", False):
                warnings.append(
                    f"part {part['name']!r} overlaps {', '.join(overlap)}; tighten the mask "
                    "or set allowOverlap for intentional joint coverage"
                )
            for pixel in visible_by_part[part["name"]]:
                claimed.setdefault(pixel, part["name"])

    valid_targets = set(names) | {"base"}
    for index, frame in enumerate(frames):
        if not isinstance(frame, dict):
            fail(f"frame {index + 1} must be an object")
        if rig_version >= 2:
            allowed_frame_keys = {
                "phase", "root", "transforms", "contacts", "zOverrides", "underlay",
                "overlay", "hold", "ik",
            }
            if rig_version >= 3:
                allowed_frame_keys.add("pose")
            unexpected_frame_keys = set(frame) - allowed_frame_keys
            if unexpected_frame_keys:
                fail(
                    f"frame {index + 1} has unsupported keys: "
                    f"{', '.join(sorted(unexpected_frame_keys))}"
                )
        elif "ik" in frame:
            fail(f"frame {index + 1} uses IK but rigVersion is not 2")
        validate_transform(frame.get("root", {}), f"frame {index + 1} root", root=True)
        transforms = frame.get("transforms", {})
        if not isinstance(transforms, dict):
            fail(f"frame {index + 1} transforms must be an object")
        unknown = set(transforms) - valid_targets
        if unknown:
            fail(f"frame {index + 1} transforms unknown parts: {', '.join(sorted(unknown))}")
        for target, transform in transforms.items():
            validate_transform(
                transform, f"frame {index + 1} transform {target!r}",
                rig_version=rig_version,
            )
        for command_key in ("underlay", "overlay"):
            commands = frame.get(command_key, [])
            if not isinstance(commands, list):
                fail(f"frame {index + 1} {command_key} must be an array")
            validate_draw_commands(commands, f"frame {index + 1} {command_key}", palette)
        if rig_version >= 2:
            if "hold" in frame and not isinstance(frame["hold"], bool):
                fail(f"frame {index + 1} hold must be true or false")
            z_overrides = frame.get("zOverrides", {})
            if not isinstance(z_overrides, dict):
                fail(f"frame {index + 1} zOverrides must be an object")
            unknown_z = set(z_overrides) - valid_targets
            if unknown_z:
                fail(f"frame {index + 1} zOverrides unknown parts: {', '.join(sorted(unknown_z))}")
            for target, value in z_overrides.items():
                z_value = finite_number(value, f"frame {index + 1} zOverrides {target!r}")
                if not z_value.is_integer():
                    fail(f"frame {index + 1} zOverrides {target!r} must be an integer")
                z_overrides[target] = int(z_value)
            frame["zOverrides"] = z_overrides
            apply_ik_constraints(parts, frame, index, width, height)
            validate_world_rotation_support(parts, frame, index)
            parse_contacts(frame, index, part_by_name)

    base, layers = prepare_layers(source, width, height, parts, rig_version)
    if rig_version >= 3:
        validate_no_residual_bone_pixels(base, width, height, parts)
    matrices_by_frame = [resolve_frame_matrices(parts, frame) for frame in frames]
    validate_articulated_motion(spec, names, frames, parts, matrices_by_frame)
    canvases = []
    for frame_index, frame in enumerate(frames):
        canvas, _ = render_frame(
            width, height, base, layers, parts, frame, palette,
            spec["baseZ"], rig_version,
        )
        if not visible_pixel_count(canvas.data):
            fail(f"frame {frame_index + 1} renders completely transparent")
        canvases.append(canvas)
    rendered_frame_hashes = [
        hashlib.sha256(bytes(canvas.data)).hexdigest() for canvas in canvases
    ]
    quality = {
        "uniqueRenderedFrames": len(set(rendered_frame_hashes)),
        "renderedFrameHashes": rendered_frame_hashes,
    }
    if rig_version >= 2:
        validate_v2_motion_quality(
            spec, width, height, source, parts, frames, base, layers, matrices_by_frame, canvases,
        )
        quality["mechanics"] = "passed"

    master_hash = decoded_master_hash
    if "masterHash" in spec:
        recorded_hash = spec["masterHash"]
        if not (
            isinstance(recorded_hash, str)
            and len(recorded_hash) == 64
            and all(character in "0123456789abcdef" for character in recorded_hash)
        ):
            fail("masterHash must be a 64-character lowercase SHA-256 hexadecimal string")
        if recorded_hash != master_hash:
            fail("locked master hash changed; create a new rig revision instead of rendering drifted artwork")
    spec["rigVersion"] = rig_version
    spec["planningMode"] = "ai-rig-deterministic-render"
    spec["masterHash"] = master_hash
    return names, frames, parts, warnings, master_hash, base, layers, quality, canvases


def read_previous_manifest(path):
    ensure_regular_or_missing(path, "generation manifest")
    if not path.exists():
        return {}
    try:
        value = strict_json_loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, ValueError):
        return {}
    return value if isinstance(value, dict) else {}


def manifest_output_paths(workspace, manifest, name, category, rig_relative):
    if not (
        manifest.get("name") == name
        and manifest.get("category") == category
        and manifest.get("rig") == rig_relative
    ):
        return None
    files = manifest.get("files")
    if not isinstance(files, list) or not (1 <= len(files) <= 32):
        fail("active rig manifest has an invalid frame list")
    paths = []
    seen = set()
    expected_parent = Path("assets") / category
    for index, value in enumerate(files):
        if not isinstance(value, str) or not value:
            fail(f"active rig manifest frame {index + 1} must be a workspace-relative path")
        relative = Path(value)
        expected_relative = expected_parent / f"{name}_{index + 1:02d}.png"
        if (
            relative.is_absolute()
            or relative != expected_relative
        ):
            fail("active rig manifest frames must be the exact ordered output sequence")
        path = safe_output_path(workspace, relative, f"active frame {index + 1}")
        if path in seen:
            fail("active rig manifest contains duplicate frame paths")
        seen.add(path)
        paths.append(path)
    return paths


def fsync_directory(path):
    if os.name == "nt":
        return
    descriptor = os.open(path, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def stage_text(directory, prefix, content, mode=None):
    descriptor, temporary_name = tempfile.mkstemp(prefix=prefix, suffix=".tmp", dir=str(directory))
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        if mode is not None:
            os.chmod(temporary, mode & 0o777)
        return temporary
    except BaseException:
        if temporary.exists() or temporary.is_symlink():
            temporary.unlink()
        raise


def atomic_write_text(path, content, mode=None):
    temporary = stage_text(path.parent, f".{path.name}-", content, mode)
    try:
        os.replace(temporary, path)
        fsync_directory(path.parent)
    finally:
        if temporary.exists() or temporary.is_symlink():
            temporary.unlink()


def remove_exact_tree(path):
    try:
        if path is not None and path.exists() and path.is_dir() and not path.is_symlink():
            shutil.rmtree(path)
            return True
    except OSError:
        return False
    return True


def commit_render(
    workspace,
    spec_path,
    source_path,
    original_spec_bytes,
    spec,
    canvases,
    manifest,
    output_paths,
):
    name = manifest["name"]
    category = manifest["category"]
    rig_relative = manifest["rig"]
    destination = safe_output_path(workspace, Path("assets") / category, "output asset")
    if destination.exists() and not destination.is_dir():
        fail("output asset directory must be a directory")
    destination.mkdir(parents=True, exist_ok=True)
    destination = destination.resolve()
    if not destination.is_relative_to(workspace):
        fail("output asset directory must stay inside the workspace")

    metadata = safe_output_path(workspace, Path(".sprite-studio"), "Sprite Studio metadata")
    if metadata.exists() and not metadata.is_dir():
        fail("Sprite Studio metadata must be a directory")
    metadata.mkdir(parents=True, exist_ok=True)
    metadata = metadata.resolve()
    if not metadata.is_relative_to(workspace):
        fail("Sprite Studio metadata directory must stay inside the workspace")

    manifest_record = safe_output_path(
        workspace, Path(".sprite-studio") / "last-generation.json", "generation manifest",
    )
    ensure_regular_or_missing(manifest_record, "generation manifest")
    ensure_regular_or_missing(spec_path, "rig record")
    for index, path in enumerate(output_paths):
        ensure_regular_or_missing(path, f"output frame {index + 1}")
        if path.resolve() == source_path.resolve() or (
            path.exists() and source_path.exists() and os.path.samefile(path, source_path)
        ):
            fail(f"output frame {index + 1} cannot replace the locked source master")

    previous = read_previous_manifest(manifest_record)
    previous_paths = manifest_output_paths(
        workspace, previous, name, category, rig_relative,
    )
    collisions = [path for path in output_paths if path.exists()]
    if collisions and previous_paths is None:
        fail("output frames exist but are not owned by this exact rig")
    old_active_paths = []
    if previous_paths is not None and collisions:
        for index, path in enumerate(previous_paths):
            ensure_regular_or_missing(path, f"active frame {index + 1}")
            if not path.exists():
                fail("the active rig output set is incomplete; restore it before rendering")
            if path.resolve() == source_path.resolve() or os.path.samefile(path, source_path):
                fail("the active rig output set aliases the locked source master")
        if any(path not in previous_paths for path in collisions):
            fail("an output frame collides with a file outside this rig's active result")
        old_active_paths = previous_paths

    transaction_root = safe_output_path(
        workspace, Path(".sprite-studio") / "transactions", "render transaction directory",
    )
    if transaction_root.exists() and not transaction_root.is_dir():
        fail("render transaction path must be a directory")
    transaction_root.mkdir(parents=True, exist_ok=True)
    transaction_dir = Path(tempfile.mkdtemp(prefix=f"{name}-", dir=str(transaction_root)))
    frame_stage = Path(tempfile.mkdtemp(prefix=f".{name}-stage-", dir=str(destination)))
    preserve_transaction = False
    try:
        staged_frames = []
        expected_hashes = manifest["quality"].get("renderedFrameHashes", [])
        if len(expected_hashes) != len(canvases):
            fail("validated render did not produce one raster hash per output frame")
        for index, (canvas, expected_hash) in enumerate(zip(canvases, expected_hashes)):
            staged = frame_stage / output_paths[index].name
            canvas.save_png(staged)
            with staged.open("rb") as handle:
                os.fsync(handle.fileno())
            staged_width, staged_height, staged_pixels, _ = load_png(staged)
            staged_hash = hashlib.sha256(bytes(staged_pixels)).hexdigest()
            if (
                staged_width != canvas.width
                or staged_height != canvas.height
                or staged_hash != expected_hash
            ):
                fail(f"staged output frame {index + 1} failed deterministic hash verification")
            staged_frames.append(staged)

        normalized_spec = strict_json_dumps(spec, indent=2) + "\n"
        manifest_text = strict_json_dumps(manifest, indent=2) + "\n"
        staged_spec = stage_text(
            transaction_dir,
            "normalized-rig-",
            normalized_spec,
            spec_path.stat().st_mode,
        )
        manifest_mode = manifest_record.stat().st_mode if manifest_record.exists() else None
        staged_manifest = stage_text(
            transaction_dir, "manifest-", manifest_text, manifest_mode,
        )

        backup_root = transaction_dir / "backups"
        frame_backups = backup_root / "frames"
        state_backups = backup_root / "state"
        frame_backups.mkdir(parents=True)
        state_backups.mkdir(parents=True)
        fsync_directory(transaction_dir)
        fsync_directory(backup_root)
        backups = {}
        original_existence = {}
        mutation_targets = list(dict.fromkeys(old_active_paths + output_paths + [spec_path, manifest_record]))
        for index, target in enumerate(mutation_targets):
            exists = target.exists()
            original_existence[target] = exists
            if not exists:
                continue
            backup_parent = frame_backups if target in old_active_paths else state_backups
            backup = (
                backup_parent / target.name
                if target in old_active_paths
                else backup_parent / f"{index:03d}-{target.name}"
            )
            copy_synced(target, backup)
            backups[target] = backup
        fsync_directory(frame_backups)
        fsync_directory(state_backups)
        fsync_directory(backup_root)

        journal_targets = []
        for target in mutation_targets:
            backup = backups.get(target)
            journal_targets.append({
                "path": str(target.relative_to(workspace)),
                "existed": original_existence[target],
                "backup": str(backup.relative_to(transaction_dir)) if backup else None,
                "sha256": sha256_file(backup) if backup else None,
            })
        journal = {
            "state": "prepared",
            "name": name,
            "rig": rig_relative,
            "outputs": [str(path.relative_to(workspace)) for path in output_paths],
            "frameStage": str(frame_stage.relative_to(workspace)),
            "targets": journal_targets,
            "createdAt": datetime.now(timezone.utc).isoformat(),
        }
        journal_path = transaction_dir / "journal.json"
        atomic_write_text(journal_path, strict_json_dumps(journal, indent=2) + "\n")

        if spec_path.read_bytes() != original_spec_bytes:
            fail("rig JSON changed while the render was being prepared; retry from the new revision")
        if sha256_file(source_path) != manifest["masterHash"]:
            fail("source master changed while the render was being prepared; retry from the new revision")
        for target, backup in backups.items():
            if not target.exists() or sha256_file(target) != sha256_file(backup):
                fail(f"{target.name} changed while the render was being prepared")
        for target, existed in original_existence.items():
            if not existed and (target.exists() or target.is_symlink()):
                fail(f"{target.name} appeared while the render was being prepared")

        try:
            journal["state"] = "committing"
            atomic_write_text(journal_path, strict_json_dumps(journal, indent=2) + "\n")
            for staged, target in zip(staged_frames, output_paths):
                os.replace(staged, target)
            for stale in old_active_paths:
                if stale not in output_paths:
                    stale.unlink()
            fsync_directory(destination)
            os.replace(staged_spec, spec_path)
            fsync_directory(spec_path.parent)
            os.replace(staged_manifest, manifest_record)
            fsync_directory(metadata)
        except BaseException as error:
            preserve_transaction = True
            rollback_errors = []
            for target in reversed(mutation_targets):
                try:
                    backup = backups.get(target)
                    if backup is not None:
                        atomic_restore(backup, target)
                    elif not original_existence[target] and (target.exists() or target.is_symlink()):
                        if target.is_file() or target.is_symlink():
                            target.unlink()
                        else:
                            raise OSError(f"cannot remove unexpected rollback target {target}")
                except BaseException as rollback_error:
                    rollback_errors.append(f"{target.name}: {rollback_error}")
            try:
                fsync_directory(destination)
                fsync_directory(metadata)
                fsync_directory(spec_path.parent)
            except OSError as rollback_error:
                rollback_errors.append(f"directory sync: {rollback_error}")
            if rollback_errors:
                fail(
                    "render commit failed and rollback needs manual recovery from "
                    f"{transaction_dir.relative_to(workspace)}: {'; '.join(rollback_errors)}"
                )
            preserve_transaction = False
            fail(f"render commit failed before activation and was rolled back: {error}")

        journal["state"] = "committed"
        preserve_transaction = True
        try:
            atomic_write_text(journal_path, strict_json_dumps(journal, indent=2) + "\n")
        except BaseException:
            pass
        else:
            preserve_transaction = False
        if old_active_paths:
            preserve_transaction = True
            try:
                archive_parent = safe_output_path(
                    workspace,
                    Path(".sprite-studio") / "versions" / "rigs" / name,
                    "rig archive",
                )
                if archive_parent.exists() and not archive_parent.is_dir():
                    raise OSError("rig archive parent must be a directory")
                archive_parent.mkdir(parents=True, exist_ok=True)
                archive_name = (
                    datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S%fZ-")
                    + transaction_dir.name.rsplit("-", 1)[-1]
                )
                archive = safe_output_path(
                    workspace,
                    archive_parent.relative_to(workspace) / archive_name,
                    "rig archive",
                )
                os.replace(frame_backups, archive)
                fsync_directory(archive_parent)
            except BaseException:
                pass
            else:
                preserve_transaction = False
    finally:
        try:
            if not remove_exact_tree(frame_stage):
                preserve_transaction = True
            if not preserve_transaction:
                remove_exact_tree(transaction_dir)
        except BaseException:
            pass


def main():
    validate_only = len(sys.argv) == 3 and sys.argv[1] == "--validate"
    check_only = len(sys.argv) == 3 and sys.argv[1] == "--check"
    if not (len(sys.argv) == 2 or validate_only or check_only):
        fail("usage: python3 .sprite-studio/sprite_rig.py [--check|--validate] RIG.json")
    workspace = Path.cwd().resolve()
    spec_path = Path(sys.argv[2] if (validate_only or check_only) else sys.argv[1])
    if not spec_path.is_absolute():
        spec_path = workspace / spec_path
    resolved_spec_path = spec_path.resolve()
    rig_root = (workspace / ".sprite-studio" / "rigs").resolve()
    if not resolved_spec_path.is_relative_to(rig_root) or spec_path.is_symlink():
        fail("rig JSON must be a regular file under .sprite-studio/rigs")
    spec_path = resolved_spec_path
    try:
        original_spec_bytes = spec_path.read_bytes()
        spec = strict_json_loads(original_spec_bytes)
    except OSError as error:
        fail(f"cannot read rig JSON: {error}")
    except UnicodeError:
        fail("rig JSON must be valid UTF-8")
    except json.JSONDecodeError as error:
        fail(f"invalid rig JSON at line {error.lineno}, column {error.colno}")
    except ValueError as error:
        fail(f"invalid rig JSON: {error}")
    if not isinstance(spec, dict):
        fail("rig JSON must contain one top-level object")
    source = spec.get("source", "")
    if not isinstance(source, str) or not source.strip():
        fail("source must be a non-empty workspace-relative PNG path")
    source_path = safe_source(workspace, source)
    width, height, source, decoded_master_hash = load_png(source_path)
    if not (8 <= width <= 512 and 8 <= height <= 512):
        fail("source canvas must be between 8 and 512 pixels per side")
    name = slug(spec.get("name", source_path.stem))
    category = slug(spec.get("category", "characters"))
    if category not in {"characters", "creatures", "terrain", "props", "effects"}:
        fail("category must be characters, creatures, terrain, props, or effects")
    names, frames, parts, warnings, master_hash, base, layers, quality, canvases = validate_rig(
        spec, source_path, width, height, source, decoded_master_hash,
    )
    rig_hash = hashlib.sha256(
        strict_json_dumps(spec, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    report = {
        "valid": True,
        "rigVersion": spec["rigVersion"],
        "name": name,
        "source": str(source_path.relative_to(workspace)),
        "masterHash": master_hash,
        "rigHash": rig_hash,
        "parts": names,
        "frames": len(frames),
        "warnings": warnings,
        "quality": quality,
    }
    if spec["rigVersion"] >= 2:
        matrices_by_frame = [resolve_frame_matrices(parts, frame) for frame in frames]
        signatures = [frame_signature(parts, matrices) for matrices in matrices_by_frame]
        looping = spec.get("looping", True)
        transition_pairs = list(zip(signatures, signatures[1:]))
        if looping:
            transition_pairs.append((signatures[-1], signatures[0]))
        transition_energy = [
            round(signature_energy(first, second), 6)
            for first, second in transition_pairs
        ]
        part_by_name = {part["name"]: part for part in parts}
        planted_contacts = []
        for frame_index, (frame, matrices) in enumerate(zip(frames, matrices_by_frame)):
            for contact in frame.get("contacts", []):
                part = part_by_name[contact["part"]]
                point = apply_matrix(
                    matrices[part["name"]], part["anchors"][contact["anchor"]],
                )
                planted_contacts.append({
                    "frame": frame_index + 1,
                    "part": part["name"],
                    "anchor": contact["anchor"],
                    "x": round(point[0], 6),
                    "y": round(point[1], 6),
                })
        report["analysis"] = {
            "looping": looping,
            "rootMotion": spec.get("rootMotion"),
            "rigProfile": spec.get("rigProfile"),
            "joints": spec.get("joints", []),
            "phases": [frame.get("phase", "") for frame in frames],
            "poses": [frame.get("pose", "") for frame in frames],
            "transitionEnergyPx": transition_energy,
            "plantedContacts": planted_contacts,
        }
    if validate_only or check_only:
        if validate_only:
            safe_spec_path = spec_path.resolve()
            if not safe_spec_path.is_relative_to((workspace / ".sprite-studio" / "rigs").resolve()):
                fail("validated rig record must stay under .sprite-studio/rigs")
            if spec_path.is_symlink():
                fail("validated rig record cannot be a symbolic link")
            lock = acquire_render_lock(workspace)
            try:
                if spec_path.read_bytes() != original_spec_bytes:
                    fail("rig JSON changed while validation was running; retry the new revision")
                if sha256_file(source_path) != decoded_master_hash:
                    fail("source master changed while validation was running; retry the new revision")
                temporary = stage_text(
                    spec_path.parent,
                    f".{spec_path.name}.validate-",
                    strict_json_dumps(spec, indent=2) + "\n",
                    spec_path.stat().st_mode,
                )
                try:
                    os.replace(temporary, spec_path)
                    fsync_directory(spec_path.parent)
                finally:
                    if temporary.exists() or temporary.is_symlink():
                        temporary.unlink()
            finally:
                lock.close()
        print(strict_json_dumps(report))
        return

    output_paths = [
        safe_output_path(
            workspace,
            Path("assets") / category / f"{name}_{index + 1:02d}.png",
            f"output frame {index + 1}",
        )
        for index in range(len(frames))
    ]
    fps = spec["fps"]
    manifest = {
        "name": name,
        "category": category,
        "fps": fps,
        "files": [str(path.relative_to(workspace)) for path in output_paths],
        "rig": str(spec_path.relative_to(workspace)),
        "masterHash": master_hash,
        "rigHash": rig_hash,
        "rigVersion": spec["rigVersion"],
        "quality": quality,
        "planningMode": "ai-rig-deterministic-render",
        "generatedAt": datetime.now(timezone.utc).isoformat(),
    }
    lock = acquire_render_lock(workspace)
    try:
        try:
            commit_render(
                workspace,
                spec_path,
                source_path,
                original_spec_bytes,
                spec,
                canvases,
                manifest,
                output_paths,
            )
        except OSError as error:
            fail(f"render transaction failed before activation: {error}")
    finally:
        lock.close()
    print(strict_json_dumps(manifest))


if __name__ == "__main__":
    main()
