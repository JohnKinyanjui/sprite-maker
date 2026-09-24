#!/usr/bin/env python3
"""Dependency-free layered pixel-rig renderer for Sprite Studio.

ImageGen (or an imported asset) supplies one transparent RGBA master. A JSON rig
selects stable pixel regions and applies deterministic nearest-neighbour
transforms to those same pixels for every animation frame.
"""

import json
import hashlib
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

from sprite_rig_commit import acquire_render_lock, commit_render, safe_output_path, safe_source
from sprite_rig_json import fail, strict_json_dumps, strict_json_loads
from sprite_rig_lock import fsync_directory, sha256_file, stage_text
from sprite_rig_matrix import apply_matrix, resolve_frame_matrices
from sprite_rig_motion import frame_signature, signature_energy
from sprite_rig_raster import load_png
from sprite_rig_validate import validate_rig
from sprite_tool import slug

def main():
    validate_only = len(sys.argv) == 3 and sys.argv[1] == "--validate"
    check_only = len(sys.argv) == 3 and sys.argv[1] == "--check"
    # Runtime-only: Sprite Studio renders the latest rig this way after the
    # provider's per-request repair budget is spent. Visual-quality gate
    # failures become recorded warnings; structural checks stay fatal.
    finalize_degraded = len(sys.argv) == 3 and sys.argv[1] == "--finalize-degraded"
    if not (len(sys.argv) == 2 or validate_only or check_only or finalize_degraded):
        fail("usage: python3 .sprite-studio/sprite_rig.py [--check|--validate] RIG.json")
    workspace = Path.cwd().resolve()
    spec_path = Path(sys.argv[2] if (validate_only or check_only or finalize_degraded) else sys.argv[1])
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
    degraded = [] if finalize_degraded else None
    names, frames, parts, warnings, master_hash, base, layers, quality, canvases = validate_rig(
        spec, source_path, width, height, source, decoded_master_hash, degraded,
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
    if degraded:
        manifest["qualityWarnings"] = degraded
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

