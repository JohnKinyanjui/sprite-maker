use super::types::{
    normalize_morphology, RigBone, RigContact, RigFrame, RigPoint, RigSuggestion, RigTransform,
};

pub fn parse_rig_suggestion_text(text: &str, width: u32, height: u32) -> Option<RigSuggestion> {
    let mut candidates = extract_fenced_blocks(text, "rig-suggestion");
    if candidates.is_empty() {
        candidates = extract_fenced_json_with_points(text);
    }
    for json_text in candidates.iter().rev() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_text) {
            if let Some(suggestion) = normalize_suggestion(value, width, height, "ai") {
                return Some(suggestion);
            }
        }
    }
    None
}

/// Parse pose frames for a fixed skeleton (a parts rig). The provider only
/// answers `frames`; the exact points and bones are injected before the usual
/// normalization, so frames can only reference real bones, and the skeleton
/// itself is returned untouched.
pub(crate) fn parse_frames_for_skeleton(
    text: &str,
    width: u32,
    height: u32,
    skeleton: RigSuggestion,
) -> Option<RigSuggestion> {
    let mut candidates = extract_fenced_blocks(text, "rig-suggestion");
    candidates.extend(extract_fenced_blocks(text, "json"));
    let points = serde_json::to_value(
        skeleton
            .points
            .iter()
            .map(|point| serde_json::json!({"name": point.name, "kind": point.kind, "x": point.x, "y": point.y}))
            .collect::<Vec<_>>(),
    )
    .ok()?;
    let bones = serde_json::to_value(
        skeleton
            .bones
            .iter()
            .map(|bone| serde_json::json!({"name": bone.name, "start": bone.start_point, "end": bone.end_point, "radius": bone.radius, "parent": bone.parent, "z": bone.z}))
            .collect::<Vec<_>>(),
    )
    .ok()?;
    for json_text in candidates.iter().rev() {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json_text) else {
            continue;
        };
        let Some(object) = value.as_object_mut() else {
            continue;
        };
        object.insert("points".into(), points.clone());
        object.insert("bones".into(), bones.clone());
        object.insert("morphology".into(), serde_json::Value::String(skeleton.morphology.clone()));
        let Some(parsed) = normalize_suggestion(value, width, height, "ai") else {
            continue;
        };
        if parsed.frames.len() < 2 {
            continue;
        }
        return Some(RigSuggestion {
            frames: parsed.frames,
            reasoning: parsed.reasoning,
            source: "ai".to_string(),
            ..skeleton
        });
    }
    None
}

fn extract_fenced_blocks(text: &str, tag: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("```") {
        let after = &remaining[start + 3..];
        let Some(first_newline) = after.find('\n') else {
            break;
        };
        let info = after[..first_newline].trim();
        let body = &after[first_newline + 1..];
        let Some(end) = body.find("```") else { break };
        if info == tag || info.starts_with(&format!("{tag} ")) {
            blocks.push(body[..end].trim().to_string());
        }
        remaining = &body[end + 3..];
    }
    blocks
}

fn extract_fenced_json_with_points(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("```") {
        let after = &remaining[start + 3..];
        let Some(first_newline) = after.find('\n') else {
            break;
        };
        let info = after[..first_newline].trim().to_ascii_lowercase();
        let body = &after[first_newline + 1..];
        let Some(end) = body.find("```") else { break };
        if info == "json" || info.is_empty() {
            let candidate = body[..end].trim();
            if candidate.contains("\"points\"") && candidate.contains("\"bones\"") {
                blocks.push(candidate.to_string());
            }
        }
        remaining = &body[end + 3..];
    }
    blocks
}

fn json_f64(value: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(number) = value.get(*key).and_then(|entry| entry.as_f64()) {
            if number.is_finite() {
                return Some(number);
            }
        }
    }
    None
}

fn json_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value.get(*key).and_then(|entry| entry.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Tolerant conversion of agent JSON into a rig suggestion: coordinates are
/// clamped to the canvas, unknown references are dropped, and cycles are cut.
pub(crate) fn normalize_suggestion(
    value: serde_json::Value,
    width: u32,
    height: u32,
    source: &str,
) -> Option<RigSuggestion> {
    let morphology =
        normalize_morphology(json_string(&value, &["morphology", "profile"]).as_deref());
    let entries = value.get("points")?.as_array()?;
    if entries.is_empty() {
        return None;
    }
    let max_x = (width.saturating_sub(1)) as f64;
    let max_y = (height.saturating_sub(1)) as f64;
    let mut points = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(x) = json_f64(entry, &["x"]) else {
            continue;
        };
        let Some(y) = json_f64(entry, &["y"]) else {
            continue;
        };
        let name =
            json_string(entry, &["name", "id"]).unwrap_or_else(|| format!("point_{}", index + 1));
        points.push(RigPoint {
            id: format!("p{}", index + 1),
            name,
            kind: json_string(entry, &["kind", "type"]).unwrap_or_else(|| "joint".into()),
            x: x.clamp(0.0, max_x),
            y: y.clamp(0.0, max_y),
            confidence: json_f64(entry, &["confidence"])
                .unwrap_or(0.8)
                .clamp(0.0, 1.0),
            source: source.to_string(),
            note: json_string(entry, &["note", "notes"]),
        });
    }
    if points.is_empty() {
        return None;
    }
    let point_exists = |name: &str| points.iter().any(|point| point.name == name);
    let mut bones = Vec::new();
    if let Some(bone_entries) = value.get("bones").and_then(|entry| entry.as_array()) {
        for (index, entry) in bone_entries.iter().enumerate() {
            let Some(start) = json_string(entry, &["start", "startPoint", "from"]) else {
                continue;
            };
            let Some(end) = json_string(entry, &["end", "endPoint", "to"]) else {
                continue;
            };
            if !point_exists(&start) || !point_exists(&end) {
                continue;
            }
            let name = json_string(entry, &["name", "id"])
                .unwrap_or_else(|| format!("bone_{}", index + 1));
            bones.push(RigBone {
                id: format!("b{}", index + 1),
                name,
                start_point: start,
                end_point: end,
                radius: json_f64(entry, &["radius"]).unwrap_or(3.0).clamp(0.5, 64.0),
                parent: json_string(entry, &["parent"]).filter(|parent| !parent.is_empty()),
                z: json_f64(entry, &["z", "zIndex", "depth"])
                    .unwrap_or(5.0)
                    .round() as i64,
            });
        }
    }
    let bone_exists = |bones: &[RigBone], name: &str| bones.iter().any(|bone| bone.name == name);
    // Cut parent cycles by walking each chain once.
    for bone_index in 0..bones.len() {
        let Some(parent) = bones[bone_index].parent.clone() else {
            continue;
        };
        if parent == bones[bone_index].name || !bone_exists(&bones, &parent) {
            bones[bone_index].parent = None;
            continue;
        }
        let mut visited = std::collections::HashSet::new();
        visited.insert(bones[bone_index].name.clone());
        let mut current = Some(parent);
        let mut cyclic = false;
        while let Some(name) = current {
            if !visited.insert(name.clone()) {
                cyclic = true;
                break;
            }
            current = bones
                .iter()
                .find(|candidate| candidate.name == name)
                .and_then(|candidate| candidate.parent.clone());
        }
        if cyclic {
            bones[bone_index].parent = None;
        }
    }
    let mut frames = Vec::new();
    if let Some(frame_entries) = value.get("frames").and_then(|entry| entry.as_array()) {
        for entry in frame_entries {
            let mut transforms = Vec::new();
            if let Some(transform_entries) = entry.get("transforms").and_then(|v| v.as_array()) {
                for transform in transform_entries {
                    let Some(bone) = json_string(transform, &["bone", "name"]) else {
                        continue;
                    };
                    if !bone_exists(&bones, &bone) {
                        continue;
                    }
                    transforms.push(RigTransform {
                        bone,
                        dx: json_f64(transform, &["dx", "x"]).unwrap_or(0.0),
                        dy: json_f64(transform, &["dy", "y"]).unwrap_or(0.0),
                        rotate: json_f64(transform, &["rotate", "angle", "rotation"])
                            .unwrap_or(0.0),
                        scale_x: json_f64(transform, &["scaleX", "scale_x", "scale"])
                            .unwrap_or(1.0),
                        scale_y: json_f64(transform, &["scaleY", "scale_y", "scale"])
                            .unwrap_or(1.0),
                    });
                }
            }
            let mut contacts = Vec::new();
            if let Some(contact_entries) = entry.get("contacts").and_then(|v| v.as_array()) {
                for contact in contact_entries {
                    let Some(bone) = json_string(contact, &["bone", "name"]) else {
                        continue;
                    };
                    if !bone_exists(&bones, &bone) {
                        continue;
                    }
                    contacts.push(RigContact {
                        bone,
                        x: json_f64(contact, &["x"]).unwrap_or(0.0).clamp(0.0, max_x),
                        y: json_f64(contact, &["y"]).unwrap_or(0.0).clamp(0.0, max_y),
                        bend: json_f64(contact, &["bend"]).unwrap_or(1.0),
                    });
                }
            }
            frames.push(RigFrame {
                phase: json_string(entry, &["phase", "name"]),
                hold: entry
                    .get("hold")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                root_dx: json_f64(entry, &["rootDx", "root_dx"]).unwrap_or(0.0),
                root_dy: json_f64(entry, &["rootDy", "root_dy"]).unwrap_or(0.0),
                transforms,
                contacts,
            });
        }
    }
    let reasoning = json_string(&value, &["reasoning", "explanation", "notes"])
        .unwrap_or_else(|| "AI-suggested rig points".to_string());
    Some(RigSuggestion {
        morphology,
        points,
        bones,
        frames,
        reasoning,
        source: source.to_string(),
    })
}
