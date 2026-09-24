"""Top-level rig validation: parts, frames, residual bones, and quality gates."""

import hashlib
import math

from sprite_rig_ik import apply_ik_constraints, parse_contacts, validate_world_rotation_support
from sprite_rig_json import fail, finite_number, point_distance
from sprite_rig_matrix import resolve_frame_matrices
from sprite_rig_motion import validate_articulated_motion, validate_v2_motion_quality
from sprite_rig_profile import normalize_anchors, normalize_bone, validate_rig_profile
from sprite_rig_raster import mask_contains, prepare_layers, render_frame, visible_pixel_count
from sprite_rig_schema import (
    mesh_contains_point,
    validate_draw_commands,
    validate_mesh,
    validate_palette,
)
from sprite_tool import slug

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


def quality_gate(degraded, message):
    """Fail a visual-quality gate, or record it when finalizing a spent repair budget."""
    if degraded is None:
        fail(message)
    degraded.append(message)


def soft_quality_check(degraded, check, *args):
    """Run a visual-quality validator, recording its failure in degraded mode."""
    if degraded is None:
        return check(*args)
    try:
        check(*args)
    except SystemExit as error:
        degraded.append(str(error).removeprefix("sprite_rig: "))


def validate_rig(spec, source_path, width, height, source, decoded_master_hash, degraded=None):
    """Validate and pre-render a rig.

    `degraded` is passed only by Sprite Studio's runtime after the per-request
    repair budget is spent. Visual-quality gates (mask ownership, residual bone
    pixels, locomotion mechanics) are then collected into that list instead of
    aborting. Schema, source, bounds, IK, transparency, master-hash, and
    distinct-frame checks remain hard failures.
    """
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
                quality_gate(
                    degraded,
                    f"parts {left_name!r} and {right_name!r} overlap {pixel_count} visible pixels "
                    "but are not a declared parent-child joint",
                )
                continue
            if child.get("overlapMode") != "joint-cap":
                quality_gate(
                    degraded,
                    f"joint {parent['name']}->{child['name']} overlaps {pixel_count} visible pixels; "
                    "declare overlapMode 'joint-cap' or make the masks exclusive",
                )
                continue
            smaller_part = min(len(visible_by_part[left_name]), len(visible_by_part[right_name]))
            scale_unit = max(1.0, min(width, height) / 64.0)
            cap_limit = max(1, min(
                int(math.ceil(12 * scale_unit * scale_unit)),
                int(math.ceil(smaller_part * 0.25)),
            ))
            if pixel_count > cap_limit:
                quality_gate(
                    degraded,
                    f"joint-cap {parent['name']}->{child['name']} covers {pixel_count} pixels; "
                    f"the bounded cap limit is {cap_limit}",
                )
                continue
            attach = child["attach"]
            joint_point = parent["anchors"][attach["parentAnchor"]]
            joint_radius = max(2.5, min(width, height) / 32.0)
            distant = [
                pixel for pixel in shared_pixels
                if point_distance((pixel[0] + 0.5, pixel[1] + 0.5), joint_point) > joint_radius
            ]
            if distant:
                quality_gate(
                    degraded,
                    f"joint-cap {parent['name']}->{child['name']} contains overlap pixels "
                    f"outside the {joint_radius:.2f}px attachment neighborhood",
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
        soft_quality_check(degraded, validate_no_residual_bone_pixels, base, width, height, parts)
    matrices_by_frame = [resolve_frame_matrices(parts, frame) for frame in frames]
    soft_quality_check(
        degraded, validate_articulated_motion, spec, names, frames, parts, matrices_by_frame,
    )
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
    if degraded is not None and quality["uniqueRenderedFrames"] < 2:
        fail("an animation must render at least two distinct frames")
    if rig_version >= 2:
        soft_quality_check(
            degraded, validate_v2_motion_quality,
            spec, width, height, source, parts, frames, base, layers, matrices_by_frame, canvases,
        )
        quality["mechanics"] = "passed"
    if degraded:
        quality["mechanics"] = "degraded"
        quality["degradedChecks"] = list(degraded)

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

